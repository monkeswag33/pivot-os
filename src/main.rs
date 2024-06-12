use std::process::Command;

fn main() {
    let uefi_path = env!("UEFI_PATH");

    let mut cmd = Command::new("qemu-system-x86_64");
    cmd.args(["-m", "128M"]);
    cmd.args(["-smp", "1"]);
    cmd.args(["-serial", "stdio"]);
    cmd.args(["-bios", "OVMF.fd"]);
    cmd.arg("-no-reboot");
    cmd.arg("-no-shutdown");
    cmd.arg("-enable-kvm");
    cmd.args(["-drive", format!("file={},index=0,media=disk,format=raw", uefi_path).as_str()]);
    cmd.spawn().unwrap().wait().unwrap();
}
