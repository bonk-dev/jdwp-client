use binrw::{BinRead, BinWrite};

pub type JdwpIdSize = u8;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JdwpIdSizes {
    pub field_id_size: JdwpIdSize,
    pub method_id_size: JdwpIdSize,
    pub object_id_size: JdwpIdSize,
    pub reference_type_id_size: JdwpIdSize,
    pub frame_id_size: JdwpIdSize,
}

#[derive(Debug)]
pub struct JdwpString {
    pub string: String,
}
impl BinRead for JdwpString {
    type Args<'a> = ();

    fn read_options<R: std::io::Read + std::io::Seek>(
        reader: &mut R,
        endian: binrw::Endian,
        args: Self::Args<'_>,
    ) -> binrw::BinResult<Self> {
        let length = u32::read_options(reader, endian, args)?;
        if length == 0 {
            return Ok(JdwpString {
                string: String::from(""),
            });
        }

        let mut bytes = vec![0u8; length as usize];
        reader.read_exact(&mut bytes)?;
        Ok(JdwpString {
            string: String::from_utf8(bytes).map_err(|e| binrw::Error::Custom {
                pos: reader.stream_position().unwrap_or(0),
                err: Box::new(e),
            })?,
        })
    }
}
impl BinWrite for JdwpString {
    type Args<'a> = ();

    fn write_options<W: std::io::Write + std::io::Seek>(
        &self,
        writer: &mut W,
        endian: binrw::Endian,
        args: Self::Args<'_>,
    ) -> binrw::BinResult<()> {
        let bytes = self.string.as_bytes();
        let length = bytes.len() as u32;
        length.write_options(writer, endian, args)?;

        if length > 0 {
            return bytes.write_options(writer, endian, args);
        }
        Ok(())
    }
}
impl PartialEq<&str> for JdwpString {
    fn eq(&self, other: &&str) -> bool {
        self.string == *other
    }
}
impl PartialEq<JdwpString> for &str {
    fn eq(&self, other: &JdwpString) -> bool {
        *self == other.string
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_read_jdwp_string_empty() {
        let data = [0u8, 0u8, 0u8, 0u8]; // 0 length
        let mut cursor = Cursor::new(&data);
        let value = JdwpString::read_be(&mut cursor).unwrap();
        assert!(value.string.is_empty());
    }

    #[test]
    fn test_read_jdwp_string() {
        let data = [0u8, 0u8, 0u8, 4u8, 74u8, 68u8, 87u8, 80u8]; // 4 length, "JDWP"
        let mut cursor = Cursor::new(&data);
        let value = JdwpString::read_be(&mut cursor).unwrap();
        assert_eq!(value.string, "JDWP");
    }

    #[test]
    fn test_write_jdwp_string() {
        let value = JdwpString {
            string: String::from("JDWP"),
        };
        let mut buffer: Vec<u8> = vec![];
        let mut cursor = Cursor::new(&mut buffer);
        value.write_be(&mut cursor).unwrap();

        let expected = [0u8, 0u8, 0u8, 4u8, 74u8, 68u8, 87u8, 80u8]; // 4 length, "JDWP"
        assert_eq!(buffer, expected);
    }

    #[test]
    fn test_write_jdwp_string_empty() {
        let value = JdwpString {
            string: String::from(""),
        };
        let mut buffer: Vec<u8> = vec![];
        let mut cursor = Cursor::new(&mut buffer);
        value.write_be(&mut cursor).unwrap();

        let expected = [0u8, 0u8, 0u8, 0u8]; // 0 length
        assert_eq!(buffer, expected);
    }
}
