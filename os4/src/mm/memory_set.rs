// 实现逻辑段与地址空间的模块
// 操作系统通过对不同页表的管理，来完成对不同应用和操作系统自身所在的虚拟内存，以及虚拟内存与物理内存映射关系的全面管理。
// 这种管理是建立在 地址空间 的抽象上，用来表明正在运行的应用或内核自身所在执行环境中的可访问的内存空间。

use super::{frame_alloc, frame_remain_num, FrameTracker};
use super::{PTEFlags, PageTable, PageTableEntry};
use super::{PhysAddr, PhysPageNum, VirtAddr, VirtPageNum};
use super::{StepByOne, VPNRange};
use crate::config::{MEMORY_END, PAGE_SIZE, TRAMPOLINE, TRAP_CONTEXT, USER_STACK_SIZE};
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use lazy_static::*;
use riscv::register::satp;
use spin::Mutex;

// 全是从ld里导入过来的
extern "C" {
    fn stext();
    fn etext();
    fn srodata();
    fn erodata();
    fn sdata();
    fn edata();
    fn sbss_with_stack();
    fn ebss();
    fn ekernel();
    fn strampoline();
}

lazy_static! {
    // 建内核地址空间的全局实例
    pub static ref KERNEL_SPACE: Arc<Mutex<MemorySet>> =
        Arc::new(Mutex::new(MemorySet::new_kernel()));
}


// 定义地址空间的结构，由一个页表和一些逻辑段组成，是一系列有关联的不一定连续的逻辑段，
// 这种关联一般是指这些逻辑段组成的虚拟内存空间与一个运行的程序（目前把一个运行的程序称为任务，后续会称为进程）绑定，
// 即这个运行的程序对代码和数据的直接访问范围限制在它关联的虚拟地址空间之内。
// 注意 PageTable 下挂着所有多级页表的节点所在的物理页帧，而每个 MapArea 下则挂着对应逻辑段中的数据所在的物理页帧，
// 这两部分合在一起构成了一个地址空间所需的所有物理页帧。
// 这同样是一种 RAII 风格，当一个地址空间 MemorySet 生命周期结束后，这些物理页帧都会被回收。
pub struct MemorySet {
    page_table: PageTable,
    areas: Vec<MapArea>,
}

impl MemorySet {

    // 新建一个空的地址空间
    pub fn new_bare() -> Self {
        Self {
            page_table: PageTable::new(),
            areas: Vec::new(),
        }
    }


    // 生成地址空间的token,就是生成其根页表的token,所以调用根页表的方法,取地址号拼上标志位
    pub fn token(&self) -> usize {
        self.page_table.token()
    }

    // insert_framed_area 方法调用 push ，可以在当前地址空间插入一个 Framed 方式映射到物理内存的逻辑段。
    pub fn insert_framed_area(
        &mut self,
        start_va: VirtAddr,
        end_va: VirtAddr,
        permission: MapPermission,
    ) {
        self.push(
            MapArea::new(start_va, end_va, MapType::Framed, permission),
            None,
        );
    }

    // push 方法可以在当前地址空间插入一个新的逻辑段 map_area 
    // 如果它是以 Framed 方式映射到物理内存，还可以可选地在那些被映射到的物理页帧上写入一些初始化数据 data
    fn push(&mut self, mut map_area: MapArea, data: Option<&[u8]>) {
        map_area.map(&mut self.page_table);
        if let Some(data) = data {
            map_area.copy_data(&mut self.page_table, data);
        }
        self.areas.push(map_area);
    }

    // 跳板代码地址加入页表里,跳板代码也就是之前的trap代码
    fn map_trampoline(&mut self) {
        // 只调用加页表方法,不用分配页帧写数据什么的,因为本来就在内存里有了
        self.page_table.map(
            VirtAddr::from(TRAMPOLINE).into(), // TRAMPOLINE是只把跳板放在虚拟地址空间最顶部,
            // 所有虚拟地址空间都这么放,那在转换的时候就不会造成指令无法桉顺序进行了
            PhysAddr::from(strampoline as usize).into(), // 物理地址对应ld的那片地址
            PTEFlags::R | PTEFlags::X, // 可读可执行
        );
    }

    // 生成内核的地址空间,在mm初始化的时候被调用,主要是为现有的内核部分内存构建一个虚拟的地址空间概念
    // 方便一会儿那token设置到satp寄存器里
    pub fn new_kernel() -> Self {
        // 先创建一个空的地址空间,它由根页表和各逻辑段组成,先都置零
        let mut memory_set = Self::new_bare();
        // 将跳板代码地址加入内核地址空间的页表里,跳板代码地址本来就在ld中排布并且导出过位置符号了
        // 就连内核也要这样映射一下才能平滑,内核其它地方都是恒等映射的,但是这里也给映射到最高处了
        memory_set.map_trampoline();


        // 将内核各段加入内核地址空间,剩下的段全都是恒等映射,
        // 恒定映射我们这里已经用枚举弄了抽象了,到时候map的时候可以根据恒等来处理
        // CPU依旧是按同一种方式查表,只不过我们在维护表的时候用恒等的方式进行维护罢了
        info!(".text [{:#x}, {:#x})", stext as usize, etext as usize);
        info!(".rodata [{:#x}, {:#x})", srodata as usize, erodata as usize);
        info!(".data [{:#x}, {:#x})", sdata as usize, edata as usize);
        info!(
            ".bss [{:#x}, {:#x})",
            sbss_with_stack as usize, ebss as usize
        );
        info!("mapping .text section");
        memory_set.push(
            MapArea::new(
                (stext as usize).into(),
                (etext as usize).into(),
                MapType::Identical,
                MapPermission::R | MapPermission::X,
            ),
            None,
        );
        info!("mapping .rodata section");
        memory_set.push(
            MapArea::new(
                (srodata as usize).into(),
                (erodata as usize).into(),
                MapType::Identical,
                MapPermission::R,
            ),
            None,
        );
        info!("mapping .data section");
        memory_set.push(
            MapArea::new(
                (sdata as usize).into(),
                (edata as usize).into(),
                MapType::Identical,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );
        info!("mapping .bss section");
        memory_set.push(
            MapArea::new(
                (sbss_with_stack as usize).into(),
                (ebss as usize).into(),
                MapType::Identical,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );
        info!("mapping physical memory");
        memory_set.push(
            MapArea::new(
                (ekernel as usize).into(),
                MEMORY_END.into(),
                MapType::Identical,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );
        // 返回内核地址空间
        memory_set
    }

    // 为分配内存的系统调用提供支持
    pub fn mmap(&mut self, start: usize, len: usize, port: usize) -> isize {
        if (port & !0b0000_0111 != 0) || (port & 0b0000_0111 == 0) {return -1;}
        let va_start = VirtAddr::from(start);
        let va_end = VirtAddr::from(start + len);
        if va_start.page_offset() != 0 { return -1; }
        let mut map_perm = MapPermission::U;
        if port & 0b0000_0001 == 0b0000_0001 {
            map_perm |= MapPermission::R;
        }
        if port & 0b0000_0010 == 0b0000_0010 {
            map_perm |= MapPermission::W;
        }
        if port & 0b0000_0100 == 0b0000_0100 {
            map_perm |= MapPermission::X;
        }
        let map_area = MapArea::new(va_start, va_end, MapType::Framed, map_perm);
        if map_area.vpn_range.get_start() > frame_remain_num() { return -1; }
        for vpn in map_area.vpn_range {
            if self.page_table.find_pte(vpn) == None { return -1; }
        }
        self.push(map_area, None);
        0
    }

    pub fn munmap(&mut self, start: usize, len: usize) -> isize {
        let vpn_start = VirtAddr::from(start).floor();
        let vpn_end = VirtAddr::from(start + len).ceil();
        let mut remain_count = usize::from(vpn_end) - usize::from(vpn_start);
        for map_area in self.areas.iter_mut() {
            if map_area.vpn_range.get_start >= vpn_start && 
            map_area.vpn_range.get_end <= vpn_end {
                map_area.unmap(self.page_table);
                remain_count -= map_area.vpn_range.len();
            }
        }
        if remain_count == 0 {
            0
        } else {
            -1
        }
    }

    // 分析应用的 ELF 文件格式的内容，解析出各数据段并生成对应的地址空间
    pub fn from_elf(elf_data: &[u8]) -> (Self, usize, usize) {
        // 新建地址空间
        let mut memory_set = Self::new_bare();
        // 插入跳板
        memory_set.map_trampoline();
        // 使用外部 crate xmas_elf 来解析传入的应用 ELF 数据并可以轻松取出各个部分。

        let elf = xmas_elf::ElfFile::new(elf_data).unwrap();
        // 得到elf头
        let elf_header = elf.header;
        // 得到魔数
        let magic = elf_header.pt1.magic;
        // 检查魔数
        assert_eq!(magic, [0x7f, 0x45, 0x4c, 0x46], "invalid elf!");
        // 得到程序头的数量，程序头部表（Program Header Table），如果存在的话，告诉系统如何创建进程映像。
        let ph_count = elf_header.pt2.ph_count();
        // 用来记录应用虚拟地址静态部分，也就各个段的结束位置
        let mut max_end_vpn = VirtPageNum(0);
        // 遍历程序头
        for i in 0..ph_count {
            let ph = elf.program_header(i).unwrap();
            // 对于LOAD类型，表明它有被内核加载的必要，进行加载操作
            if ph.get_type().unwrap() == xmas_elf::program::Type::Load {
                // 用ph.virtual_addr()和ph.mem_size()查看ELF期望这一区域在应用虚拟地址空间中的位置
                let start_va: VirtAddr = (ph.virtual_addr() as usize).into();
                let end_va: VirtAddr = ((ph.virtual_addr() + ph.mem_size()) as usize).into();
                // 用ph_flags查看ELF期望这一区域的权限
                // 首先肯定是用户可访问的
                let mut map_perm = MapPermission::U;
                let ph_flags = ph.flags();
                // 分别看各种权限
                if ph_flags.is_read() {
                    map_perm |= MapPermission::R;
                }
                if ph_flags.is_write() {
                    map_perm |= MapPermission::W;
                }
                if ph_flags.is_execute() {
                    map_perm |= MapPermission::X;
                }
                // 可以为任务的这个段创建逻辑段了
                let map_area = MapArea::new(start_va, end_va, MapType::Framed, map_perm);
                max_end_vpn = map_area.vpn_range.get_end();
                // 压入任务的地址空间
                memory_set.push(
                    map_area,
                    // 压入的同时附带数据
                    Some(&elf.input[ph.offset() as usize..(ph.offset() + ph.file_size()) as usize]),
                );
            }
        }
        // 刚才记录了静态部分的结束位置，接下来在静态部分的上方再分配以一个逻辑段作为用户栈
        // 页号转换为地址，取整4K对齐
        let max_end_va: VirtAddr = max_end_vpn.into();
        // 设置栈的最下界
        let mut user_stack_bottom: usize = max_end_va.into();
        // 搞一个保护页，有虚页面无实际页帧，好在栈溢出的时候trap
        user_stack_bottom += PAGE_SIZE;
        // 设置栈最上界
        let user_stack_top = user_stack_bottom + USER_STACK_SIZE;
        // 用户栈压入地址空间
        memory_set.push(
            MapArea::new(
                user_stack_bottom.into(),
                user_stack_top.into(),
                MapType::Framed,
                MapPermission::R | MapPermission::W | MapPermission::U,
            ),
            None,
        );
        // 压入trap上下文段，这部分config文件中给出了地址
        memory_set.push(
            MapArea::new(
                TRAP_CONTEXT.into(),
                TRAMPOLINE.into(),
                MapType::Framed,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );
        // 返回地址空间、用户栈底位置、应用程序入口点
        (
            memory_set,
            user_stack_top,
            elf.header.pt2.entry_point() as usize,
        )
    }
    
    // token 会按照 satp CSR 格式要求 构造一个无符号 64 位无符号整数，使得其分页模式为 SV39 ，
    // 且将当前多级页表的根节点所在的物理页号填充进去。
    // 我们将这个值写入当前 CPU 的 satp CSR ，从这一刻开始 SV39 分页模式就被启用了，
    // 而且 MMU 会使用内核地址空间的多级页表进行地址转换。

    // 拿到一个地址空间,生成对应的token放进satp中
    pub fn activate(&self) {
        // 生成token,也就是生成根页表的token,取地址号拼上标志位
        let satp = self.page_table.token();
        // 放进satp
        unsafe {
            satp::write(satp);
            core::arch::asm!("sfence.vma");
        }
    }
    pub fn translate(&self, vpn: VirtPageNum) -> Option<PageTableEntry> {
        self.page_table.translate(vpn)
    }
}

// 我们以逻辑段 MapArea 为单位描述一段连续地址的虚拟内存。
// 所谓逻辑段，就是指地址区间中的一段实际可用（即 MMU 通过查多级页表可以正确完成地址转换）的地址连续的虚拟地址区间，
// 该区间内包含的所有虚拟页面都以一种相同的方式映射到物理页帧，具有可读/可写/可执行等属性。
pub struct MapArea {
    vpn_range: VPNRange, // 描述一段虚拟页号的连续区间，表示该逻辑段在地址区间中的位置和长度。
    data_frames: BTreeMap<VirtPageNum, FrameTracker>, // 当逻辑段采用 MapType::Framed 方式映射到物理内存的时候， 
    // data_frames 是一个保存了该逻辑段内的每个虚拟页面和它被映射到的物理页帧 FrameTracker 的一个键值对容器 BTreeMap 中，
    // 这些物理页帧被用来存放实际内存数据而不是作为多级页表中的中间节点。
    map_type: MapType, // 物理页帧与虚拟页之间的映射关系，有恒等映射（S级）和依靠页表映射（U级）两种
    map_perm: MapPermission, // 控制该逻辑段的访问方式，它是页表项标志位 PTEFlags 的一个子集，仅保留 U/R/W/X 四个标志位
}

impl MapArea {

    // new 方法可以新建一个逻辑段结构体，注意传入的起始/终止虚拟地址会分别被
    // 下取整/上取整为虚拟页号并传入迭代器 vpn_range 中
    pub fn new(
        start_va: VirtAddr,
        end_va: VirtAddr,
        map_type: MapType,
        map_perm: MapPermission,
    ) -> Self {
        let start_vpn: VirtPageNum = start_va.floor();
        let end_vpn: VirtPageNum = end_va.ceil();
        Self {
            vpn_range: VPNRange::new(start_vpn, end_vpn),
            data_frames: BTreeMap::new(),
            map_type,
            map_perm,
        }
    }

    // 对逻辑段中的单个虚拟页面进行映射, 添加到多级页表中
    pub fn map_one(&mut self, page_table: &mut PageTable, vpn: VirtPageNum) {
        let ppn: PhysPageNum;
        match self.map_type {
            MapType::Identical => {
                ppn = PhysPageNum(vpn.0);
            }
            MapType::Framed => {
                let frame = frame_alloc().unwrap();
                ppn = frame.ppn;
                self.data_frames.insert(vpn, frame);
            }
        }
        let pte_flags = PTEFlags::from_bits(self.map_perm.bits).unwrap();
        page_table.map(vpn, ppn, pte_flags);
    }
    #[allow(unused)]
    // 对逻辑段中的单个虚拟页面进行映射, 从多级页表中删除
    pub fn unmap_one(&mut self, page_table: &mut PageTable, vpn: VirtPageNum) {
        #[allow(clippy::single_match)]
        match self.map_type {
            MapType::Framed => {
                self.data_frames.remove(&vpn);
            }
            _ => {}
        }
        page_table.unmap(vpn);
    }

    // 将当前逻辑段到物理内存的映射从传入的该逻辑段所属的地址空间的多级页表中加入
    // 遍历逻辑段中的所有虚拟页面，并以每个虚拟页面为单位依次在多级页表中进行键值对的插入
    pub fn map(&mut self, page_table: &mut PageTable) {
        for vpn in self.vpn_range {
            self.map_one(page_table, vpn);
        }
    }

    #[allow(unused)]
    // 将当前逻辑段到物理内存的映射从传入的该逻辑段所属的地址空间的多级页表中删除
    // 遍历逻辑段中的所有虚拟页面，并以每个虚拟页面为单位依次在多级页表中进行键值对的删除
    pub fn unmap(&mut self, page_table: &mut PageTable) {
        for vpn in self.vpn_range {
            self.unmap_one(page_table, vpn);
        }
    }

    // 将切片 data 中的数据拷贝到当前逻辑段实际被内核放置在的各物理页帧上，从而在地址空间中通过该逻辑段就能访问这些数据。
    pub fn copy_data(&mut self, page_table: &mut PageTable, data: &[u8]) {
        assert_eq!(self.map_type, MapType::Framed);
        let mut start: usize = 0;
        let mut current_vpn = self.vpn_range.get_start();
        let len = data.len();
        loop {
            let src = &data[start..len.min(start + PAGE_SIZE)];
            let dst = &mut page_table
                .translate(current_vpn)
                .unwrap()
                .ppn()
                .get_bytes_array()[..src.len()];
            dst.copy_from_slice(src);
            start += PAGE_SIZE;
            if start >= len {
                break;
            }
            current_vpn.step();
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
// 逻辑段的映射类型，恒等映射或依靠页表
pub enum MapType {
    Identical,
    Framed,
}

bitflags! {
    // 逻辑段的访问方式
    pub struct MapPermission: u8 {
        const R = 1 << 1;
        const W = 1 << 2;
        const X = 1 << 3;
        const U = 1 << 4;
    }
}




#[allow(unused)]
// 测试
pub fn remap_test() {
    let mut kernel_space = KERNEL_SPACE.lock();
    let mid_text: VirtAddr = ((stext as usize + etext as usize) / 2).into();
    let mid_rodata: VirtAddr = ((srodata as usize + erodata as usize) / 2).into();
    let mid_data: VirtAddr = ((sdata as usize + edata as usize) / 2).into();
    assert!(!kernel_space
        .page_table
        .translate(mid_text.floor())
        .unwrap()
        .writable());
    assert!(!kernel_space
        .page_table
        .translate(mid_rodata.floor())
        .unwrap()
        .writable());
    assert!(!kernel_space
        .page_table
        .translate(mid_data.floor())
        .unwrap()
        .executable());
    info!("remap_test passed!");
}
