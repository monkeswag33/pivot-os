#![no_std]

use uefi::table::{boot::MemoryMap, Runtime, SystemTable};

pub struct BootInfo {
    pub fb: FrameBufferInfo,
    pub acpi: ACPIInfo,
    pub runtime: SystemTable<Runtime>,
    pub mmap: MemoryMap<'static>
}

#[repr(C, packed)]
pub struct SDTHeader {
    pub signature: [char; 4],
    pub length: u32,
    pub revision: u8,
    pub checksum: u8,
    pub oemid: [char; 6],
    pub oem_table_id: [char; 8],
    pub oem_revision: u32,
    pub creator_id: u32,
    pub creator_revision: u32
}

#[repr(C, packed)]
pub struct XSDP {
    pub signature: [char; 8],
    pub checksum: u8,
    pub oemid: [char; 6],
    pub revision: u8,
    pub rsdt_address: u32,
    pub length: u32,
    pub xsdt_address: u64,
    pub ext_checksum: u8,
    rsv: [u8; 3]
}

pub struct ACPIInfo {
    pub xsdt: bool,
    pub address: &'static XSDP
}

pub struct FrameBufferInfo {
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub fb: *mut u8
}
