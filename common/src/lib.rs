#![no_std]

use framebuffer::FrameBuffer;
use x86_64::{structures::paging::PhysFrame, PhysAddr};
use uefi::table::boot::MemoryMap;

pub mod framebuffer;
pub mod logger;
pub mod mem;

pub const HIGHER_HALF: u64 = 0xFFFF800000000000;
pub const PAGE_SIZE: u64 = 4096;

#[derive(Debug)]
pub struct BootInfo {
    pub framebuffer: Option<FrameBuffer>,
    pub rsdp: Option<PhysAddr>,
    pub mmap: MemoryMap<'static>,
    pub ffa: PhysFrame
}