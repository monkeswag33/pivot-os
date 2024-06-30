QEMU_FLAGS := -m 128M -smp 2 -enable-kvm -serial stdio -bios OVMF.fd -no-reboot -no-shutdown -d int
HEADER_FILES := $(shell find include -type f -name "*.h")

all: KERNEL_LDFLAGS += -S -s
all: BOOT_LDFLAGS += -S -s
all: build/os.img
include src/boot/Makefile
include src/kernel/Makefile

.PHONY = run debug clean

run: KERNEL_LDFLAGS += -S -s
run: BOOT_LDFLAGS += -S -s
run: base-run

debug: KERNEL_CFLAGS += -g -DDEBUG
debug: QEMU_FLAGS += -s
debug: base-run

base-run: build/os.img
	qemu-system-x86_64 $(QEMU_FLAGS) -drive file=$<,index=0,media=disk,format=raw

build/os.img: build/BOOTX64.efi build/kernel.elf
	./efi2img.sh $^ $@

clean:
	rm -rf build/
