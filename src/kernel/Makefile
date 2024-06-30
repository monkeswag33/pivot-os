KERNEL_TARGET := build/kernel.elf
KERNEL_C_SRC := $(shell find src/kernel -type f -name "*.c")
KERNEL_C_OBJ := $(patsubst src/kernel/%.c, build/kernel/%.o, $(KERNEL_C_SRC))
KERNEL_ASM_SRC := $(shell find src/kernel -type f -name "*.asm")
KERNEL_ASM_OBJ := $(patsubst src/kernel/%.asm, build/kernel/%.o, $(KERNEL_ASM_SRC))
KERNEL_FONT_OBJ := build/fonts/default.o
KERNEL_CFLAGS := -Iinclude/ \
				 -ffreestanding \
				 -nostdlib \
				 -Wall \
				 -Wextra \
				 -mno-red-zone \
				 -mno-sse \
				 -mcmodel=large \
				 -fno-stack-protector
KERNEL_LINKER_FILE := src/kernel/linker.ld
KERNEL_LDFLAGS := -T $(KERNEL_LINKER_FILE) -z max-page-size=0x1000
KERNEL_FLAGS := -Iinclude/ -fPIC -Bsymbolic -ffreestanding -nostdlib -target x86_64-unknown-elf \
	-Wall -Wextra -mno-red-zone -mno-sse -mcmodel=large -fuse-ld=lld \
	-fno-stack-protector -T$(KERNEL_LINKER_FILE) -z max-page-size=0x1000

$(KERNEL_TARGET): $(KERNEL_C_SRC) $(KERNEL_ASM_OBJ) $(KERNEL_FONT_OBJ)
	@mkdir -p build
	$(CC) $(KERNEL_FLAGS) $(KERNEL_C_SRC) $(KERNEL_FONT_OBJ) -o $@
	# $(CC) $(KERNEL_LDFLAGS) -o $@ $^

build/kernel/%.o: src/kernel/%.c $(HEADER_FILES)
	@mkdir -p $(@D)
	$(CC) $(KERNEL_CFLAGS) -c $(filter %.c,$^) -o $@

build/kernel/%.o: src/kernel/%.asm
	@mkdir -p $(@D)
	nasm -felf64 $< -o $@

build/fonts/%.o: fonts/%.psf
	@mkdir -p $(@D)
	objcopy -O elf64-x86-64 -B i386 -I binary $< $@