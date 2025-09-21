use binrw::{BinRead, binrw, binwrite};

use crate::{
    ClassStatus, JdwpIdSize, JdwpIdSizes, JdwpString, JdwpStringSlice, TypeTag, binrw_enum,
};

binrw_enum! {
    #[repr(u16)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Command {
        VirtualMachineVersion =                 (1 << 8) | 1,
        VirtualMachineClassesBySignature =      (1 << 8) | 2,
        VirtualMachineAllClasses =              (1 << 8) | 3,
        VirtualMachineAllThreads =              (1 << 8) | 4,
        VirtualMachineTopLevelThreadGroups =    (1 << 8) | 5,
        VirtualMachineDispose =                 (1 << 8) | 6,
        VirtualMachineIDSizes =                 (1 << 8) | 7,
        VirtualMachineSuspend =                 (1 << 8) | 8,
        VirtualMachineResume =                  (1 << 8) | 9,
    }
}

#[binrw]
#[brw(big)]
pub struct CommandPacketHeader {
    pub length: u32,
    pub id: u32,
    pub flags: u8,
    pub command: Command,
}
impl CommandPacketHeader {
    pub fn get_length() -> usize {
        return 4 + 4 + 1 + 2;
    }
}

#[binrw]
#[brw(big)]
pub struct ReplyPacketHeader {
    pub length: u32,
    pub id: u32,
    pub flags: u8,
    pub error_code: u16,
}
impl ReplyPacketHeader {
    pub fn default() -> Self {
        ReplyPacketHeader {
            length: 0,
            id: 0xFFFFFFFF,
            flags: 0,
            error_code: 0,
        }
    }
    pub fn get_length() -> usize {
        return 4 + 4 + 1 + 2;
    }
    pub fn is_success(&self) -> bool {
        return self.error_code == 0;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VariableLengthId {
    pub value: u64,
}
impl BinRead for VariableLengthId {
    type Args<'a> = JdwpIdSize;

    fn read_options<R: std::io::Read + std::io::Seek>(
        reader: &mut R,
        endian: binrw::Endian,
        args: Self::Args<'_>,
    ) -> binrw::BinResult<Self> {
        // TODO: Support non-power-of-2 sizes if needed
        let val: u64 = match args {
            1 => u8::read_options(reader, endian, ())? as u64,
            2 => u16::read_options(reader, endian, ())? as u64,
            4 => u32::read_options(reader, endian, ())? as u64,
            8 => u64::read_options(reader, endian, ())?,
            _ => {
                return binrw::BinResult::Err(binrw::Error::Custom {
                    pos: reader.stream_position().unwrap_or(0),
                    err: Box::new("Unsupported variable size ID"),
                });
            }
        };

        Ok(VariableLengthId { value: val })
    }
}

#[binrw]
#[brw(big)]
#[derive(Debug)]
pub struct VersionReply {
    pub description: JdwpString,
    pub jdwp_major: i32,
    pub jdwp_minor: i32,
    pub vm_version: JdwpString,
    pub vm_name: JdwpString,
}

// ====== BEGIN VirtualMachine_ClassesBySignature ======

#[binwrite]
#[br(big)]
#[derive(Debug)]
pub struct ClassesBySignatureOut<'a> {
    pub signature: JdwpStringSlice<'a>,
}

#[derive(Debug)]
pub struct ClassesBySignatureReplyClass {
    pub ref_type_tag: TypeTag,
    pub type_id: VariableLengthId,
    pub status: ClassStatus,
}

impl BinRead for ClassesBySignatureReplyClass {
    type Args<'a> = JdwpIdSizes;

    fn read_options<R: std::io::Read + std::io::Seek>(
        reader: &mut R,
        endian: binrw::Endian,
        args: Self::Args<'_>,
    ) -> binrw::BinResult<Self> {
        Ok(ClassesBySignatureReplyClass {
            ref_type_tag: TypeTag::read_options(reader, endian, ())?,
            type_id: VariableLengthId::read_options(reader, endian, args.reference_type_id_size)?,
            status: ClassStatus::read_options(reader, endian, ())?,
        })
    }
}

#[derive(Debug)]
pub struct ClassesBySignatureReply {
    pub classes: Vec<ClassesBySignatureReplyClass>,
}
impl BinRead for ClassesBySignatureReply {
    type Args<'a> = JdwpIdSizes;

    fn read_options<R: std::io::Read + std::io::Seek>(
        reader: &mut R,
        endian: binrw::Endian,
        args: Self::Args<'_>,
    ) -> binrw::BinResult<Self> {
        let classes_length = i32::read_options(reader, endian, ())?;
        let mut classes = Vec::with_capacity(classes_length as usize);
        for _ in 0..classes_length {
            classes.push(ClassesBySignatureReplyClass::read_options(
                reader, endian, args,
            )?);
        }

        Ok(ClassesBySignatureReply { classes })
    }
}

// ====== END VirtualMachine_ClassesBySignature ======

#[derive(Debug)]
pub struct AllClassesReplyClass {
    pub ref_type_tag: TypeTag,
    pub type_id: VariableLengthId,
    pub signature: JdwpString,
    pub status: ClassStatus,
}
impl BinRead for AllClassesReplyClass {
    type Args<'a> = JdwpIdSizes;

    fn read_options<R: std::io::Read + std::io::Seek>(
        reader: &mut R,
        endian: binrw::Endian,
        args: Self::Args<'_>,
    ) -> binrw::BinResult<Self> {
        Ok(AllClassesReplyClass {
            ref_type_tag: TypeTag::read_options(reader, endian, ())?,
            type_id: VariableLengthId::read_options(reader, endian, args.reference_type_id_size)?,
            signature: JdwpString::read_options(reader, endian, ())?,
            status: ClassStatus::read_options(reader, endian, ())?,
        })
    }
}

#[derive(Debug)]
pub struct AllClassesReply {
    pub classes: Vec<AllClassesReplyClass>,
}
impl BinRead for AllClassesReply {
    type Args<'a> = JdwpIdSizes;

    fn read_options<R: std::io::Read + std::io::Seek>(
        reader: &mut R,
        endian: binrw::Endian,
        args: Self::Args<'_>,
    ) -> binrw::BinResult<Self> {
        let classes_length = i32::read_options(reader, endian, ())?;
        let mut classes = Vec::with_capacity(classes_length as usize);
        for _ in 0..classes_length {
            classes.push(AllClassesReplyClass::read_options(reader, endian, args)?);
        }

        Ok(AllClassesReply { classes })
    }
}

// ====== BEGIN VirtualMachine_AllThreads ======
#[derive(Clone, Copy, Debug)]
pub struct AllThreadsReplyThread {
    pub thread_id: VariableLengthId,
}

impl BinRead for AllThreadsReplyThread {
    type Args<'a> = JdwpIdSizes;

    fn read_options<R: std::io::Read + std::io::Seek>(
        reader: &mut R,
        endian: binrw::Endian,
        args: Self::Args<'_>,
    ) -> binrw::BinResult<Self> {
        Ok(AllThreadsReplyThread {
            thread_id: VariableLengthId::read_options(reader, endian, args.object_id_size)?,
        })
    }
}

#[derive(Debug)]
pub struct AllThreadsReply {
    pub threads: Vec<AllThreadsReplyThread>,
}

impl BinRead for AllThreadsReply {
    type Args<'a> = JdwpIdSizes;

    fn read_options<R: std::io::Read + std::io::Seek>(
        reader: &mut R,
        endian: binrw::Endian,
        args: Self::Args<'_>,
    ) -> binrw::BinResult<Self> {
        let length = u32::read_options(reader, endian, ())?;
        let mut thread_ids = Vec::with_capacity(length as usize);
        for _ in 0..length {
            thread_ids.push(AllThreadsReplyThread::read_options(reader, endian, args)?);
        }
        Ok(AllThreadsReply {
            threads: thread_ids,
        })
    }
}
// ====== END VirtualMachine_AllThreads ======

// ====== BEGIN VirtualMachine_TopLevelThreadGroups ======
#[derive(Clone, Copy, Debug)]
pub struct TopLevelThreadGroupsReplyThreadGroup {
    pub thread_group_id: VariableLengthId,
}

impl BinRead for TopLevelThreadGroupsReplyThreadGroup {
    type Args<'a> = JdwpIdSizes;

    fn read_options<R: std::io::Read + std::io::Seek>(
        reader: &mut R,
        endian: binrw::Endian,
        args: Self::Args<'_>,
    ) -> binrw::BinResult<Self> {
        Ok(TopLevelThreadGroupsReplyThreadGroup {
            thread_group_id: VariableLengthId::read_options(reader, endian, args.object_id_size)?,
        })
    }
}

#[derive(Debug)]
pub struct TopLevelThreadGroupsReply {
    pub threads_groups: Vec<TopLevelThreadGroupsReplyThreadGroup>,
}

impl BinRead for TopLevelThreadGroupsReply {
    type Args<'a> = JdwpIdSizes;

    fn read_options<R: std::io::Read + std::io::Seek>(
        reader: &mut R,
        endian: binrw::Endian,
        args: Self::Args<'_>,
    ) -> binrw::BinResult<Self> {
        let length = u32::read_options(reader, endian, ())?;
        let mut thread_ids = Vec::with_capacity(length as usize);
        for _ in 0..length {
            thread_ids.push(TopLevelThreadGroupsReplyThreadGroup::read_options(
                reader, endian, args,
            )?);
        }
        Ok(TopLevelThreadGroupsReply {
            threads_groups: thread_ids,
        })
    }
}
// ====== END VirtualMachine_TopLevelThreadGroups ======

// ====== BEGIN VirtualMachine_IDSizes ======
#[binrw]
#[brw(big)]
#[derive(Debug)]
pub struct IdSizesReply {
    pub field_id_size: i32,
    pub method_id_size: i32,
    pub object_id_size: i32,
    pub reference_type_id_size: i32,
    pub frame_id_size: i32,
}
// ====== END VirtualMachine_IDSizes ======

#[cfg(test)]
mod tests {
    use crate::Command;
    use binrw::BinRead;
    use std::io::Cursor;

    #[test]
    fn test_deserialize_vm_version_command() {
        let data = [1u8, 1u8];
        let mut cursor = Cursor::new(&data);
        let value = Command::read_be(&mut cursor).unwrap();
        assert_eq!(value, Command::VirtualMachineVersion);
    }
}
