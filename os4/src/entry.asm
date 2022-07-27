# 放在kernel空间的最前面,802地址上
# 没有变化，依旧是设置sp来指定栈顶，然后跳到rust_main执行

# 这段是放在text节中的最前面的
    .section .text.entry
    .globl _start
_start:
    la sp, boot_stack_top
    call rust_main


# 这段是放在bss段中的,初始的内核自己的栈
    .section .bss.stack
    .globl boot_stack
boot_stack:
    .space 4096 * 16
    .globl boot_stack_top
boot_stack_top: