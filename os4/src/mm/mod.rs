// 内存管理模块，使用与RV64处理器的SV39三级页表约定
// 内核堆初始化、物理页帧管理、页表管理、地址空间都在这个模块


mod address;
mod frame_allocator;
mod heap_allocator;
mod memory_set;
mod page_table;

pub use address::{PhysAddr, PhysPageNum, VirtAddr, VirtPageNum};
use address::{StepByOne, VPNRange};
pub use frame_allocator::{frame_alloc, frame_remain_num, FrameTracker};
pub use memory_set::remap_test;
pub use memory_set::{MapPermission, MemorySet, KERNEL_SPACE};
pub use page_table::{translated_byte_buffer, translated_assign_ptr, PageTableEntry};
use page_table::{PTEFlags, PageTable};

// 初始化内核堆分配器、物理页帧分配器和内核地址空间
pub fn init() {
    // 首先初始化内核堆分配器,在bss上划一个大数组给导入的伙伴分配器,以此实例化一个静态伙伴分配器
    heap_allocator::init_heap();
    // 初始化物理页帧分配器,利用ld中的符号ekernel找到内核结束位置,然后把后面的地址全都上下取整获得物理页帧号分给分配器
    // 看来按照SV39约定,cpu真就是取物理地址的中间44位作为页帧号,而qume给我们的是从0x80000000开始的地址,那估计是80000开头的号?
    frame_allocator::init_frame_allocator();
    // 创建内核地址空间并让 CPU 开启分页模式， MMU 在地址转换的时候使用内核的多级页表，这一切均在一行之内做到
    // 首先，我们引用 KERNEL_SPACE ，这是它第一次被使用，就在此时它会被初始化
    // 接着使用 .lock()访问里面的数据
    // 最后，我们调用 MemorySet::activate, 设置satp, 使能分页模式
    KERNEL_SPACE.lock().activate();
}
