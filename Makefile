TARGET = pivotos.efi
SRCS = entry.c
ALSO = kernel.elf
CFLAGS += -I./kernel/include
include uefi/Makefile

.PHONY = run
run: $(TARGET) $(ALSO)
	uefi-run $(TARGET) -f $(ALSO)

$(ALSO): FORCE
	make -C kernel

FORCE: ;

clean: kernelclean
kernelclean:
	make -C uefi clean
	make -C kernel clean
	rm -f *.o *.efi *.elf *.so