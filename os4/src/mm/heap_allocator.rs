//全局分配器

// 内核堆大小
use crate::config::KERNEL_HEAP_SIZE;
// 使用伙伴分配器第三方库
use buddy_system_allocator::LockedHeap;

// 标注全局堆分配器，使能alloc库
#[global_allocator]
// 创建伙伴分配器全局实例,这也是内部可变,互斥锁 Mutex<T>(跨线程版的RefCell)
static HEAP_ALLOCATOR: LockedHeap = LockedHeap::empty();

// 绑定分配出错处理
#[alloc_error_handler]
// 堆分配出错直接panic
pub fn handle_alloc_error(layout: core::alloc::Layout) -> ! {
    panic!("Heap allocation error, layout = {:?}", layout);
}

// 为内核堆分配空间
static mut HEAP_SPACE: [u8; KERNEL_HEAP_SIZE] = [0; KERNEL_HEAP_SIZE];

// 初始化内核堆分配器，把刚才划定的区域给它
pub fn init_heap() {
    unsafe {
        HEAP_ALLOCATOR
            .lock()
            .init(HEAP_SPACE.as_ptr() as usize, KERNEL_HEAP_SIZE);
    }
}

// 测试
#[allow(unused)]
pub fn heap_test() {
    use alloc::boxed::Box;
    use alloc::vec::Vec;
    extern "C" {
        fn sbss();
        fn ebss();
    }
    let bss_range = sbss as usize..ebss as usize;
    let a = Box::new(5);
    assert_eq!(*a, 5);
    assert!(bss_range.contains(&(a.as_ref() as *const _ as usize)));
    drop(a);
    let mut v: Vec<usize> = Vec::new();
    for i in 0..500 {
        v.push(i);
    }
    for (i, vi) in v.iter().enumerate().take(500) {
        assert_eq!(*vi, i);
    }
    assert!(bss_range.contains(&(v.as_ptr() as usize)));
    drop(v);
    info!("heap_test passed!");
}
