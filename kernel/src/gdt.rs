use x86_64::{instructions::segmentation, registers::segmentation::Segment, structures::gdt::{Descriptor, GlobalDescriptorTable}};
static mut GDT: GlobalDescriptorTable = GlobalDescriptorTable::new();

pub fn init_gdt() {
    unsafe {
        let cs = GDT.append(Descriptor::kernel_code_segment());
        let ds = GDT.append(Descriptor::kernel_data_segment());
        GDT.append(Descriptor::user_code_segment());
        GDT.append(Descriptor::user_data_segment());
        GDT.load();
        segmentation::CS::set_reg(cs);
        segmentation::DS::set_reg(ds);
        segmentation::SS::set_reg(ds);
        segmentation::ES::set_reg(ds);
        segmentation::FS::set_reg(ds);
        segmentation::GS::set_reg(ds);
    }
}