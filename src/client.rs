use binrw::{BinRead, BinWrite};
use std::collections::HashMap;
use std::io;
use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::sync::{Mutex, oneshot};
use tokio::time::timeout;

use crate::{
    AllClassesReply, Command, CommandPacketHeader, IdSizesReply, JdwpIdSizes, ReplyPacketHeader,
    VersionReply, result,
};

pub struct JdwpClient<T> {
    writer: Arc<Mutex<WriteHalf<T>>>,
    pending_requests: Arc<Mutex<HashMap<u32, oneshot::Sender<ReplyPacket>>>>,
    packet_id: Arc<Mutex<u32>>,
    _reader_handle: tokio::task::JoinHandle<()>,
    sizes: Option<JdwpIdSizes>,
}

struct ReplyPacket {
    header: ReplyPacketHeader,
    data: Vec<u8>,
}

impl<T> JdwpClient<T>
where
    T: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    pub async fn new(mut stream: T) -> result::Result<Self> {
        Self::do_handshake(&mut stream).await?;

        let (reader, writer) = tokio::io::split(stream);

        let pending_requests = Arc::new(Mutex::new(HashMap::new()));
        let writer_arc = Arc::new(Mutex::new(writer));
        let packet_id = Arc::new(Mutex::new(0));

        // Spawn reader task
        let pending_clone = pending_requests.clone();
        let reader_handle = tokio::spawn(async move {
            Self::reader_loop(reader, pending_clone).await;
        });

        Ok(JdwpClient {
            writer: writer_arc,
            pending_requests,
            packet_id,
            _reader_handle: reader_handle,
            sizes: None,
        })
    }

    async fn reader_loop(
        mut reader: ReadHalf<T>,
        pending_requests: Arc<Mutex<HashMap<u32, oneshot::Sender<ReplyPacket>>>>,
    ) {
        loop {
            // TODO: Handle command packets coming from the VM
            match Self::read_reply_packet(&mut reader).await {
                Ok(reply_packet) => {
                    let mut pending = pending_requests.lock().await;
                    if let Some(sender) = pending.remove(&reply_packet.header.id) {
                        let _ = sender.send(reply_packet);
                    }
                }
                Err(e) => {
                    eprintln!("Reader task error: {:?}", e);
                    // Notify all pending requests about the error
                    let mut pending = pending_requests.lock().await;
                    for (_, sender) in pending.drain() {
                        let _ = sender.send(ReplyPacket {
                            header: ReplyPacketHeader::default(),
                            data: Vec::new(),
                        });
                    }
                    break;
                }
            }
        }
    }

    async fn read_reply_packet(reader: &mut ReadHalf<T>) -> result::Result<ReplyPacket> {
        // Read header
        let mut header_buffer = vec![0u8; ReplyPacketHeader::get_length()];
        reader.read_exact(&mut header_buffer).await?;

        let mut cursor = Cursor::new(&header_buffer);
        let header =
            ReplyPacketHeader::read_be(&mut cursor).map_err(|e| result::Error::ParsingError {
                message: format!("Parsing error: {:?}", e),
            })?;

        // Read data
        let data_length = header.length as usize - ReplyPacketHeader::get_length();
        let mut data = vec![0u8; data_length];
        reader.read_exact(&mut data).await?;

        Ok(ReplyPacket { header, data })
    }

    async fn write_request(
        writer: &mut WriteHalf<T>,
        header: &CommandPacketHeader,
        data: &[u8],
    ) -> result::Result<()> {
        // Write header
        let mut header_buffer = Vec::with_capacity(CommandPacketHeader::get_length());
        let mut cursor = Cursor::new(&mut header_buffer);
        header
            .write_be(&mut cursor)
            .map_err(|e| result::Error::ParsingError {
                message: format!("Serialization error: {:?}", e),
            })?;

        writer.write_all(&header_buffer).await?;
        writer.write_all(data).await?;
        writer.flush().await?;

        Ok(())
    }

    async fn next_packet_id(&self) -> u32 {
        let mut id = self.packet_id.lock().await;
        *id = id.wrapping_add(1);
        *id
    }

    async fn send_request_with_timeout(
        &self,
        command: Command,
        data: Vec<u8>,
        timeout_duration: Duration,
    ) -> result::Result<ReplyPacket> {
        let id = self.next_packet_id().await;
        let (tx, rx) = oneshot::channel();

        // Register pending request
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(id, tx);
        }

        // Create header
        let header = CommandPacketHeader {
            length: CommandPacketHeader::get_length() as u32 + data.len() as u32,
            id,
            flags: 0,
            command,
        };

        // Send request
        {
            let mut writer = self.writer.lock().await;
            Self::write_request(&mut *writer, &header, &data).await?;
        }

        // Wait for reply with timeout
        match timeout(timeout_duration, rx).await {
            Ok(Ok(reply)) => Ok(reply),
            Ok(Err(_)) => Err(result::Error::IoError(io::Error::new(
                io::ErrorKind::Other,
                "Reply channel closed",
            ))),
            Err(_) => {
                // Timeout - clean up pending request
                let mut pending = self.pending_requests.lock().await;
                pending.remove(&id);
                Err(result::Error::IoError(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Request timed out",
                )))
            }
        }
    }

    async fn send_bodyless<TReply: for<'a> BinRead<Args<'a> = ()>>(
        &self,
        cmd: Command,
        timeout_duration: Duration,
    ) -> result::Result<TReply>
    where
        for<'a> <TReply as BinRead>::Args<'a>: Default,
    {
        let reply_packet = self
            .send_request_with_timeout(cmd, Vec::new(), timeout_duration)
            .await?;

        let mut cursor = Cursor::new(&reply_packet.data);
        let reply = TReply::read_be(&mut cursor).map_err(|e| result::Error::ParsingError {
            message: format!("Binary parsing error: {:?}", e),
        })?;

        Ok(reply)
    }

    async fn send_bodyless_variable<TReply: for<'a> BinRead<Args<'a> = JdwpIdSizes>>(
        &self,
        cmd: Command,
        timeout_duration: Duration,
    ) -> result::Result<TReply> {
        let reply_packet = self
            .send_request_with_timeout(cmd, Vec::new(), timeout_duration)
            .await?;

        let mut cursor = Cursor::new(&reply_packet.data);
        let reply = TReply::read_be_args(
            &mut cursor,
            self.sizes.ok_or(result::Error::IdSizesUnknown)?,
        )
        .map_err(|e| result::Error::ParsingError {
            message: format!("Binary parsing error: {:?}", e),
        })?;

        Ok(reply)
    }

    async fn do_handshake(stream: &mut T) -> result::Result<()> {
        const HANDSHAKE_STR: &str = "JDWP-Handshake";

        let handshake_bytes = HANDSHAKE_STR.as_bytes();
        stream.write_all(handshake_bytes).await?;
        stream.flush().await?;

        let mut buffer = [0u8; 14];
        stream.read_exact(&mut buffer).await?;

        let received = std::str::from_utf8(&buffer)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid UTF-8"))?;

        if received != HANDSHAKE_STR {
            return Err(result::Error::ParsingError {
                message: format!(
                    "Invalid handshake: expected '{}', got '{}'",
                    HANDSHAKE_STR, received
                ),
            });
        }

        Ok(())
    }

    pub async fn vm_get_version(&self) -> result::Result<VersionReply> {
        self.send_bodyless(Command::VirtualMachineVersion, Duration::from_secs(5))
            .await
    }

    pub async fn vm_get_all_classes(&self) -> result::Result<AllClassesReply> {
        self.send_bodyless_variable(Command::VirtualMachineAllClasses, Duration::from_secs(5))
            .await
    }

    pub async fn vm_get_id_sizes(&self) -> result::Result<IdSizesReply> {
        self.send_bodyless(Command::VirtualMachineIDSizes, Duration::from_secs(5))
            .await
    }
    pub async fn get_id_sizes(&mut self) -> result::Result<()> {
        let sizes = self.vm_get_id_sizes().await?;
        let field_id: u8 = sizes
            .field_id_size
            .try_into()
            .map_err(|_| result::Error::IdSizesTruncated)?;
        let method_id: u8 = sizes
            .method_id_size
            .try_into()
            .map_err(|_| result::Error::IdSizesTruncated)?;
        let object_id: u8 = sizes
            .object_id_size
            .try_into()
            .map_err(|_| result::Error::IdSizesTruncated)?;
        let ref_id: u8 = sizes
            .reference_type_id_size
            .try_into()
            .map_err(|_| result::Error::IdSizesTruncated)?;
        let frame_id: u8 = sizes
            .frame_id_size
            .try_into()
            .map_err(|_| result::Error::IdSizesTruncated)?;
        self.sizes = Some(JdwpIdSizes {
            field_id_size: field_id,
            method_id_size: method_id,
            object_id_size: object_id,
            reference_type_id_size: ref_id,
            frame_id_size: frame_id,
        });
        Ok(())
    }
}
