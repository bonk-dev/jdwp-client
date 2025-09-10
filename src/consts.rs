use crate::binrw_enum;
use binrw::binrw;
use bitflags::bitflags;

#[derive(Debug, Clone, Copy)]
#[binrw]
pub struct ClassStatus(i32);
bitflags! {
    impl ClassStatus : i32 {
        const VERIFIED = 1;
        const PREPARED = 1 << 1;
        const INITIALIZED = 1 << 2;
        const ERROR = 1 << 3;
    }
}

binrw_enum! {
    #[repr(u8)]
    #[derive(Debug, Clone, Copy)]
    pub enum TypeTag {
        Class = 1,
        Interface = 2,
        Array = 3
    }
}
