//! Types related to task management
use super::TaskContext;
use crate::config::{kernel_stack_position, TRAP_CONTEXT};
use crate::mm::{MapPermission, MemorySet, PhysPageNum, VirtAddr, KERNEL_SPACE};
use crate::trap::{trap_handler, TrapContext};
use super::MAX_SYSCALL_NUM;

// 任务控制块
pub struct TaskControlBlock {
    pub task_status: TaskStatus, // 任务状态，未运行、挂起、运行中、结束
    pub task_cx: TaskContext, // 任务上下文，12个s寄存器、ra寄存器、sp寄存器
    pub memory_set: MemorySet, // 地址空间，页表、逻辑段实体
    pub trap_cx_ppn: PhysPageNum, // trap上下文的物理页帧号，也就是物理地址中间那部分
    pub base_size: usize, // 应用数据的大小，也就是在应用地址空间中从0x0开始到用户栈结束一共包含多少字节。
    // LAB1: Add whatever you need about the Task.
    pub task_syscall_times: [u32; MAX_SYSCALL_NUM], // 各种系统调用的次数
    pub task_first_running_time: Option<usize>, // 任务第一次被调度的时刻
}

impl TaskControlBlock {
    pub fn get_trap_cx(&self) -> &'static mut TrapContext {
        self.trap_cx_ppn.get_mut()
    }
    pub fn get_user_token(&self) -> usize {
        self.memory_set.token()
    }
    // 新建一个任务，得到这个任务的任务控制块
    pub fn new(elf_data: &[u8], app_id: usize) -> Self {
        // memory_set with elf program headers/trampoline/trap context/user stack
        // 先要给任务新建地址空间，使用ELF文件，按ELF期望进行布局，得到地址空间、栈指针初始位置、程序入口点
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        // 得到trap上下文的物理页号
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT).into())
            .unwrap()
            .ppn();
        // 任务状态设置为未运行
        let task_status = TaskStatus::Ready;
        // 在内核空间给应用分配个内核栈，kernel_stack_position来自config的规定
        let (kernel_stack_bottom, kernel_stack_top) = kernel_stack_position(app_id);
        KERNEL_SPACE.lock().insert_framed_area(
            kernel_stack_bottom.into(),
            kernel_stack_top.into(),
            MapPermission::R | MapPermission::W,
        );
        // 创建任务控制块
        let task_control_block = Self {
            task_status,
            task_cx: TaskContext::goto_trap_return(kernel_stack_top), // 在初始启动中，任务挂起上下文设置成ra为trap_return的地址，s是零，sp是内核栈
            // 这样看起来就好像是即将从trap中恢复时被挂起了
            // 这样还是在初次任务切换的时候就会从trap恢复过程开始执行
            memory_set,
            trap_cx_ppn,
            base_size: user_sp,
            task_syscall_times: [0; MAX_SYSCALL_NUM],
            task_first_running_time: None,
        };
        // 设置trap上下文，让挂起的程序恢复时从trap恢复到用户态执行
        let trap_cx = task_control_block.get_trap_cx();
        *trap_cx = TrapContext::app_init_context(
            entry_point, // 程序入口点
            user_sp, // 用户栈初始指针
            // 下面这仨是固定的
            KERNEL_SPACE.lock().token(), // 内核空间页表token
            kernel_stack_top, // 内核栈顶
            trap_handler as usize, // trap处理函数
        );
        task_control_block
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
/// task status: UnInit, Ready, Running, Exited
pub enum TaskStatus {
    UnInit,
    Ready,
    Running,
    Exited,
}
