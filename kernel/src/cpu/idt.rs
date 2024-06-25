use lazy_static::lazy_static;
use spin::Lazy;
use x86_64::{set_general_handler, structures::idt::{InterruptDescriptorTable, InterruptStackFrame}};

// Pushes a filler byte to preserve the stack if needed
// macro_rules! emit_err_code {
//     (true) => { "" };
//     (false) => { "push 0" };
// }

// macro_rules! isr {
//     ($err_code: literal, $num:literal) => {
//         paste::item! {
//             #[naked]
//             unsafe extern "C" fn [< isr $num >] () {
//                 asm!(
//                     emit_err_code!($err_code),
//                     "push {}",
//                     "push rax",
//                     "push rbx",
//                     "push rcx",
//                     "push rdx",
//                     "push rbp",
//                     "push rsi",
//                     "push rdi",
//                     "push r8",
//                     "push r9",
//                     "push r10",
//                     "push r11",
//                     "push r12",
//                     "push r13",
//                     "push r14",
//                     "push r15",
//                     "mov rdi, rsp",
//                     "cld",
//                     "call exception_handler",
//                     "mov rsp, rax",
//                     "pop r15",
//                     "pop r14",
//                     "pop r13",
//                     "pop r12",
//                     "pop r11",
//                     "pop r10",
//                     "pop r9",
//                     "pop r8",
//                     "pop rdi",
//                     "pop rsi",
//                     "pop rbp",
//                     "pop rdx",
//                     "pop rcx",
//                     "pop rbx",
//                     "pop rax",
//                     "add rsp, 16",
//                     "iretq",
//                     const $num,
//                     options(noreturn)
//                 );
//             }
//         }
//     };
// }

// isr!(false, 0);
// isr!(false, 1);
// isr!(false, 2);
// isr!(false, 3);
// isr!(false, 4);
// isr!(false, 5);
// isr!(false, 6);
// isr!(false, 7);
// isr!(true, 8);
// isr!(false, 9);
// isr!(true, 10);
// isr!(true, 11);
// isr!(true, 12);
// isr!(true, 13);
// isr!(true, 14);
// isr!(false, 15);
// isr!(false, 16);
// isr!(true, 17);
// isr!(false, 18);
// isr!(false, 19);
// isr!(false, 20);
// isr!(false, 21);
// isr!(false, 22);
// isr!(false, 23);
// isr!(false, 24);
// isr!(false, 25);
// isr!(false, 26);
// isr!(false, 27);
// isr!(false, 28);
// isr!(false, 29);
// isr!(true, 30);
// isr!(false, 31);

fn exception_handler(stack_frame: InterruptStackFrame, idx: u8, err_code: Option<u64>) {
    // log::error!("Received interrupt {}", idx);
    // log::debug!("Stack frame: {:?}", stack_frame);
    // log::debug!("Err code: {:?}", err_code);
    unreachable!();
}

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        set_general_handler!(&mut idt, exception_handler, 0..32);
        idt
    };
}

pub fn init_idt() {
    x86_64::instructions::interrupts::disable();
    IDT.load();
}