use spin::Lazy;
use x86_64::{instructions::segmentation, registers::segmentation::Segment, structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector}, PrivilegeLevel};

static GDT: Lazy<GlobalDescriptorTable> = Lazy::new(|| {
    let mut gdt = GlobalDescriptorTable::new();
    gdt.append(Descriptor::kernel_code_segment());
    gdt.append(Descriptor::kernel_data_segment());
    gdt.append(Descriptor::user_code_segment());
    gdt.append(Descriptor::user_data_segment());
    gdt
});

pub fn init_gdt() {
    GDT.load();
    let cs = SegmentSelector::new(1, PrivilegeLevel::Ring0);
    let ds = SegmentSelector::new(2, PrivilegeLevel::Ring0);
    unsafe {
        segmentation::CS::set_reg(cs);
        segmentation::DS::set_reg(ds);
        segmentation::SS::set_reg(ds);
        segmentation::ES::set_reg(ds);
        segmentation::FS::set_reg(ds);
        segmentation::GS::set_reg(ds);
    }
    log::info!("Initialized GDT");
}