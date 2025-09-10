/// Macro to implement BinRead + BinWrite for repr(xx) enums
#[macro_export]
macro_rules! binrw_enum {
    (
        #[repr($ty:ty)]
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $($variant:ident = $value:expr),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[repr($ty)]
        $vis enum $name {
            $($variant = $value),*
        }

        impl binrw::BinRead for $name {
            type Args<'a> = ();

            fn read_options<R: binrw::io::Read + binrw::io::Seek>(
                reader: &mut R,
                endian: binrw::Endian,
                _: Self::Args<'_>
            ) -> binrw::BinResult<Self> {
                let val = <$ty>::read_options(reader, endian, ())?;
                match val {
                    $(x if x == $value => Ok(Self::$variant),)*
                    other => Err(binrw::Error::AssertFail {
                        pos: reader.stream_position()?,
                        message: format!(
                            "Invalid value {} for enum {}",
                            other,
                            stringify!($name)
                        ),
                    }),
                }
            }
        }

        impl binrw::BinWrite for $name {
            type Args<'a> = ();

            fn write_options<W: binrw::io::Write + binrw::io::Seek>(
                &self,
                writer: &mut W,
                endian: binrw::Endian,
                _: Self::Args<'_>
            ) -> binrw::BinResult<()> {
                let val: $ty = match self {
                    $(Self::$variant => $value),*
                };
                val.write_options(writer, endian, ())
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use binrw::{BinRead, BinWrite};
    use std::io::Cursor;

    binrw_enum! {
        #[repr(u8)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum TestSet {
            Value1 = 1,
            Value2 = 2,
            Value3 = 3,
        }
    }
    binrw_enum! {
        #[repr(u16)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum TestSet16 {
            Value1 = 1,
            Value2 = 2,
            Value3 = 3,
        }
    }

    #[test]
    fn test_enum_read_valid() {
        let data = [2u8]; // binary data for Value2
        let mut cursor = Cursor::new(&data);
        let value = TestSet::read_be(&mut cursor).unwrap();
        assert_eq!(value, TestSet::Value2);
    }

    #[test]
    fn test_enum_read_invalid() {
        let data = [99u8]; // not mapped to any variant
        let mut cursor = Cursor::new(&data);
        let result = TestSet::read_be(&mut cursor);
        assert!(result.is_err());
    }

    #[test]
    fn test_enum_write() {
        let mut buffer = Cursor::new(Vec::new());
        TestSet::Value3.write_be(&mut buffer).unwrap();
        assert_eq!(buffer.into_inner(), vec![3u8]);
    }

    #[test]
    fn test_enum_read_valid_u16() {
        let data = [0u8, 2u8]; // binary data for Value2
        let mut cursor = Cursor::new(&data);
        let value = TestSet16::read_be(&mut cursor).unwrap();
        assert_eq!(value, TestSet16::Value2);
    }

    #[test]
    fn test_enum_read_invalid_u16() {
        let data = [0u8, 99u8]; // not mapped to any variant
        let mut cursor = Cursor::new(&data);
        let result = TestSet16::read_be(&mut cursor);
        assert!(result.is_err());
    }

    #[test]
    fn test_enum_write_u16() {
        let mut buffer = Cursor::new(Vec::new());
        TestSet16::Value3.write_be(&mut buffer).unwrap();
        assert_eq!(buffer.into_inner(), vec![0u8, 3u8]);
    }
}
