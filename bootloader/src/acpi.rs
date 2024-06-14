use uefi::{prelude::*, table::cfg::{ACPI2_GUID, ACPI_GUID}};

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

pub fn configure_acpi(st: &SystemTable<Boot>) -> ACPIInfo {
    let mut config_entries = st.config_table().iter();
    let table = config_entries
        .find(|t| t.guid == ACPI2_GUID)
        .or_else(|| config_entries.find(|t| t.guid == ACPI_GUID));
    let table = table.expect("Couldn't find an ACPI table");
    let address = table.address as *const u8;

    if !unsafe { validate_table(address, 20) } {
        panic!("RSDP was not valid");
    }

    let xsdp = unsafe { &*(address as *const XSDP) };
    if xsdp.revision == 2 && !unsafe { validate_table(address, xsdp.length) } {
        panic!("XSDP was not valid");
    }
    log::info!("Found ACPI tables");

    ACPIInfo { xsdt: xsdp.revision == 2, address: xsdp }
}

unsafe fn validate_table(table: *const u8, size: u32) -> bool {
    let mut sum = 0u8;
    for i in 0..size {
        sum = sum.wrapping_add(*table.add(i.try_into().unwrap()));
    }
    sum == 0
}

impl SDTHeader {
    pub unsafe fn validate(self) -> bool {
        validate_table(&self as *const SDTHeader as *const u8, self.length)
    }
}