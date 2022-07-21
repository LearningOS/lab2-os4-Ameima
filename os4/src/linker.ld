/*内核自身的链接空间脚本*/
/*ELF是[ELF头-程序头-各段数据-节头表],所以之后再用rust-readobj掐头去尾去掉元数据得到排列好的段数据*/

/*输出平台riscv*/
OUTPUT_ARCH(riscv)
/*入口点在符号_start*/
ENTRY(_start)
/*设置一个名为基地址的全局变量,值为0x80200000*/
BASE_ADDRESS = 0x80200000;

SECTIONS
{
    /*当前输出位置为基地址*/
    . = BASE_ADDRESS;
    /*这个位置定义一个符号skernel,也就是start kernel*/
    skernel = .;

    /*再定义一个符号stext,也就是start text*/
    stext = .;

    /*这说明最贴着基地址放置的是kernel的空间,而kernel最前面的是text段*/


    /*放置text段在最前面*/
    .text : {
        /*先放置入口点段,在entry.asm中定义*/
        *(.text.entry)
        . = ALIGN(4K);/*进行4K对其*/

        /*再放置跳板段*/
        strampoline = .;
        *(.text.trampoline);
        . = ALIGN(4K);/*进行4K对其*/

        /*其余text段内容*/
        *(.text .text.*)
    }

    . = ALIGN(4K);/*进行4K对其*/

    /*标识符end text*/
    etext = .;

    /*标识符start rodata,read only只读的全局常量*/
    srodata = .;
    .rodata : {
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
    }

    . = ALIGN(4K);
    erodata = .;

    /*初始化过的非const全局变量*/
    sdata = .;
    .data : {
        *(.data .data.*)
        *(.sdata .sdata.*)
    }

    . = ALIGN(4K);
    edata = .;

    /*初始化为0的全局变量*/
    sbss_with_stack = .;
    .bss : {
        *(.bss.stack)
        sbss = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
    }

    . = ALIGN(4K);
    ebss = .;

    /*内核空间结束*/
    ekernel = .;

    /*不链接调试信息段*/
    /DISCARD/ : {
        *(.eh_frame)
    }
}