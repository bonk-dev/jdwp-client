use std::collections::HashMap;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// A mock stream that implements AsyncRead + AsyncWrite for testing
pub struct MockStream {
    read_data: std::collections::VecDeque<u8>,
    write_data: Vec<u8>,
    waker: Option<Waker>,
    responses: HashMap<Vec<u8>, Vec<u8>>,
    default_response: Option<Vec<u8>>,
    closed: bool,
}

impl MockStream {
    /// Create a new mock stream
    pub fn new() -> Self {
        Self {
            read_data: std::collections::VecDeque::new(),
            write_data: Vec::new(),
            waker: None,
            responses: HashMap::new(),
            default_response: None,
            closed: false,
        }
    }

    /// Add data to be read by the client
    pub fn add_read_data(&mut self, data: &[u8]) {
        self.read_data.extend(data);
    }

    /// Set up automatic response: when `input` is written, `output` will be available to read
    pub fn set_response(&mut self, input: Vec<u8>, output: Vec<u8>) {
        self.responses.insert(input, output);
    }

    /// Set a default response for any input not specifically mapped
    pub fn set_default_response(&mut self, output: Vec<u8>) {
        self.default_response = Some(output);
    }

    /// Get all data that was written to this stream
    pub fn written_data(&self) -> &[u8] {
        &self.write_data
    }

    /// Clear all written data (useful between test operations)
    pub fn clear_written(&mut self) {
        self.write_data.clear();
    }

    /// Check for automatic responses based on what was written
    fn check_responses(&mut self) {
        // Check if what was written matches any response patterns
        for (input, output) in &self.responses.clone() {
            if self.write_data.ends_with(input) {
                self.read_data.extend(output);
                if let Some(waker) = self.waker.take() {
                    waker.wake();
                }
                return;
            }
        }

        // Check default response
        if let Some(default) = &self.default_response {
            if !self.write_data.is_empty() {
                self.read_data.extend(default);
                if let Some(waker) = self.waker.take() {
                    waker.wake();
                }
            }
        }
    }
}

impl AsyncRead for MockStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        println!("mock poll");

        if self.closed && self.read_data.is_empty() {
            return Poll::Ready(Ok(()));
        }

        let to_read = std::cmp::min(buf.remaining(), self.read_data.len());
        if to_read == 0 {
            self.waker = Some(cx.waker().clone());
            return Poll::Pending;
        }

        println!("mock reading");
        for _ in 0..to_read {
            if let Some(byte) = self.read_data.pop_front() {
                buf.put_slice(&[byte]);
            }
        }

        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for MockStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        if self.closed {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "Stream is closed",
            )));
        }

        self.write_data.extend_from_slice(buf);
        println!("MockStream write: {:x?}", buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        self.check_responses();
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        self.closed = true;
        Poll::Ready(Ok(()))
    }
}

/// Builder for creating mock streams with predefined behavior
pub struct MockStreamBuilder {
    responses: HashMap<Vec<u8>, Vec<u8>>,
    default_response: Option<Vec<u8>>,
    initial_read_data: Vec<u8>,
}

impl MockStreamBuilder {
    pub fn new() -> Self {
        Self {
            responses: HashMap::new(),
            default_response: None,
            initial_read_data: Vec::new(),
        }
    }

    /// Add a response from byte slices
    pub fn response_bytes(mut self, input: &[u8], output: &[u8]) -> Self {
        self.responses.insert(input.to_vec(), output.to_vec());
        self
    }

    /// Add JDWP-Handshake as input&output
    pub fn with_jdwp_handshake(self) -> Self {
        let handshake_bytes = "JDWP-Handshake".as_bytes();
        self.response_bytes(handshake_bytes, handshake_bytes)
    }

    /// Add VirtualMachine_IDSizes request&response with request id: 1 (for init)
    pub fn with_initial_id_sizes(self) -> Self {
        const RESPONSE_LENGTH: u8 = 11 + 5 * 4; // header + 5 int32s
        const ID: u8 = 1; // id: 1
        const FLAGS: u8 = 0x80; // reply
        const ERROR_CODE: u8 = 0x00; // success
        const ALL_ID_LENGTH: u8 = 0x8; // each id will be 8 bytes

        self.response_bytes(
            &[0x1, 0x7], // VirtualMachine | IDSizes
            &[
                0x0,
                0x0,
                0x0,
                RESPONSE_LENGTH,
                0x0,
                0x0,
                0x0,
                ID,
                FLAGS,
                0x0,
                ERROR_CODE,
                // 5 int32s, all equal to 8 (big-endian)
                0x0,
                0x0,
                0x0,
                ALL_ID_LENGTH,
                0x0,
                0x0,
                0x0,
                ALL_ID_LENGTH,
                0x0,
                0x0,
                0x0,
                ALL_ID_LENGTH,
                0x0,
                0x0,
                0x0,
                ALL_ID_LENGTH,
                0x0,
                0x0,
                0x0,
                ALL_ID_LENGTH,
            ],
        )
    }

    /// Set default response for any unmatched input
    pub fn default_response(mut self, output: Vec<u8>) -> Self {
        self.default_response = Some(output);
        self
    }

    /// Build the mock stream
    pub fn build(self) -> MockStream {
        let mut stream = MockStream::new();

        for (input, output) in self.responses {
            stream.set_response(input, output);
        }

        if let Some(default) = self.default_response {
            stream.set_default_response(default);
        }

        if !self.initial_read_data.is_empty() {
            stream.add_read_data(&self.initial_read_data);
        }

        stream
    }
}

impl Default for MockStreamBuilder {
    fn default() -> Self {
        Self::new().with_jdwp_handshake().with_initial_id_sizes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_basic_read_write() {
        let mut stream = MockStream::new();
        stream.add_read_data(b"hello");

        // Test writing
        stream.write_all(b"world").await.unwrap();
        stream.flush().await.unwrap();
        assert_eq!(stream.written_data(), b"world");

        // Test reading
        let mut buffer = [0; 5];
        stream.read_exact(&mut buffer).await.unwrap();
        assert_eq!(&buffer, b"hello");
    }

    #[tokio::test]
    async fn test_automatic_responses() {
        let mut stream = MockStreamBuilder::new()
            .response_bytes(b"ping", b"pong")
            .response_bytes(b"hello", b"world")
            .build();

        stream.write_all(b"ping").await.unwrap();
        stream.flush().await.unwrap();

        let mut buffer = [0; 4];
        stream.read_exact(&mut buffer).await.unwrap();
        assert_eq!(&buffer, b"pong");

        stream.clear_written();
        stream.write_all(b"hello").await.unwrap();
        stream.flush().await.unwrap();

        let mut buffer = [0; 5];
        stream.read_exact(&mut buffer).await.unwrap();
        assert_eq!(&buffer, b"world");
    }

    #[tokio::test]
    async fn test_default_response() {
        let mut stream = MockStreamBuilder::new()
            .default_response(b"default".to_vec())
            .build();

        stream.write_all(b"anything").await.unwrap();
        stream.flush().await.unwrap();

        let mut buffer = [0; 7];
        stream.read_exact(&mut buffer).await.unwrap();
        assert_eq!(&buffer, b"default");
    }
}
