// 关闭std与main
#![no_std]
#![no_main]

// 启用panic信息和内核堆分配失败处理函数
#![feature(panic_info_message)]
#![feature(alloc_error_handler)]

// 新增bitflags，用于方便的操作bit位标志
#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate log;

extern crate alloc;

#[macro_use]
mod console;
mod config;
mod lang_items;
mod loader;
mod logging;
mod mm;
mod sbi;
mod sync;
mod syscall;
mod task;
mod timer;
mod trap;

// 入口点文件和app都放进来一起编译链接

core::arch::global_asm!(include_str!("entry.asm"));
// app都整体放进data段,使用.incbin伪指令在编译时接入
core::arch::global_asm!(include_str!("link_app.S"));

// 清零bss,一会儿内核堆栈都放这里面,堆虽然是用static mut声明数组分配的,但也是在.bss段里
fn clear_bss() {
    extern "C" {
        fn sbss();
        fn ebss();
    }
    unsafe {
        core::slice::from_raw_parts_mut(sbss as usize as *mut u8, ebss as usize - sbss as usize)
            .fill(0);
    }
}

#[no_mangle]
pub fn rust_main() -> ! {
    clear_bss();
    logging::init();
    println!("[kernel] Hello, world!");
    // 新增，内存管理模块初始化,启动内核堆,启动帧分配器,启动分页模式
    mm::init();
    println!("[kernel] back to world!");
    // 新增, 检查内核地址空间的多级页表是否被正确设置
    mm::remap_test();
    // 设置stvec寄存器指向trap处理函数的地址
    trap::init();
    // 通过 sie 寄存器中的 seie 位，对中断信号是否接收进行控制。设置为接受
    trap::enable_timer_interrupt();
    // 设置mtimecmp寄存器为10ms后触发中断
    timer::set_next_trigger();
    // 启动第一个任务,构造好任务上下文和trap上下文并触发还原
    task::run_first_task();
    panic!("Unreachable in rust_main!");
}
