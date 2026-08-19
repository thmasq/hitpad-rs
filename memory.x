MEMORY
{
    FLASH : ORIGIN = 0x08000000, LENGTH =   64K
    DMA_RAM : ORIGIN = 0x24000000, LENGTH = 32K
    RAM     : ORIGIN = 0x24008000, LENGTH = 424K
}

SECTIONS
{
    .dma_buffer (NOLOAD) : ALIGN(32) {
        *(.sram3 .sram3.*);
    } > DMA_RAM
}

