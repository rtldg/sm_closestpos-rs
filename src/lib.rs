
use std::cell::RefCell;
use std::error::Error;
use std::rc::Rc;
use std::os::raw::c_void;
//use std::vec;
use core::arch::x86::*;

use sm_ext::{
    SMExtension, IExtension, IExtensionInterface, IPluginContext,
    IShareSys, IHandleSys, HandleId, HandleType,
    TryIntoPlugin, HandleError,
    ICellArray,
    TypeAccess,
    IdentityTokenPtr,
    //SMInterfaceApi,
    native, register_natives, cell_t,
};

use kiddo::KdTree;
use kiddo::distance::squared_euclidean;

use float_ord::FloatOrd;

/*
#[cfg(target_os = "linux")]
#[link(kind="dylib", name="sourcemod.logic")]
extern "C" {
    #[no_mangle]
    pub static g_pCoreIdent: *const c_void;
}
*/

#[repr(align(16))]
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
struct Point {
    dims: [FloatOrd<f32>; 3],
    data: i32,
}

//#[derive(Debug)]
struct ClosestPosContainer {
    tree: KdTree<f32, i32, 3>,
    startidx: i32,
    //points: Vec<Point>,
}

impl<'ctx> TryIntoPlugin<'ctx> for ClosestPosContainer {
    type Error = HandleError;

    fn try_into_plugin(self, ctx: &'ctx IPluginContext) -> Result<cell_t, Self::Error> {
        let object = Rc::new(RefCell::new(self));
        let handle = MyExtension::closestpos_handle_type().create_handle(object, ctx.get_identity(), None)?;

        Ok(handle.into())
    }
}

// public native ClosestPos(ArrayList input, int offset=0, int startidx=0, int count=2147483646);
#[native]
fn native_closestpos_create(_ctx: &IPluginContext, arraylist: HandleId, offset: i32, startidx: Option<i32>, count: Option<i32>) -> Result<ClosestPosContainer, Box<dyn Error>> {
    //println!(">>> ClosestPos.ClosestPos({:?}, {:?}, {:?}, {:?})", arraylist, offset, startidx, count);

    let startidx = startidx.unwrap_or(0);

    if offset < 0 {
        return Err(format!("Offset must be 0 or greater (given {})", offset).into());
    }

    let offset = offset as usize;

    let arraylist = MyExtension::arraylist_handle_type().read_handle_ez(arraylist)?;

    let size = arraylist.size() as i32;
    let mut count = count.unwrap_or(size);

    if startidx < 0 || startidx > (size-1) {
        return Err(format!("startidx ({}) must be >=0 and less than the ArrayList size ({})", startidx, size).into());
    }

    if count < 1 {
        return Err(format!("count must be 1 or greater (given {})", count).into());
    }

    count = std::cmp::min(count, size-startidx);

    let bs = arraylist.blocksize();
    let mut blk = arraylist.at(startidx as usize); // start off behind

    let mut container = ClosestPosContainer {
        tree: KdTree::new(),
        //points: Vec::with_capacity(count as usize),
        startidx: startidx,
    };
    //unsafe {
    //    container.points.set_len(count as usize);
    //}

   // let mut cur = container.points.as_mut_ptr();

    for i in startidx..count {
        unsafe {
            let floatsss = blk.add(offset); // TODO optimize out...

            /*
            let pos = _mm_loadu_ps(floatsss as *const f32);
            _mm_store_ps(cur as *mut f32, pos);
            (*cur).data = i;
            cur = cur.add(1);
            */

            let pos = std::mem::transmute::<*mut cell_t, &[f32; 3]>(
                floatsss
            );
            container.tree.add(pos, i)?;

            blk = blk.add(bs); // this is better than a virtualcall to .at() everytime...
        }
    }

    //container.points.sort_by(|a, b| a.partial_cmp(b).unwrap());
    /*
    for v in container.points.iter() {
        let pos = unsafe {
            std::mem::transmute::<&[FloatOrd<f32>; 3], &[f32; 3]>(
                &v.dims
            )
        };
        container.tree.add(pos, v.data)?;
    }
    */

    Ok(container)
}

// public native int Find(float pos[3]);
#[native]
fn native_closestpos_find(ctx: &IPluginContext, handleid: HandleId, pos_addr: cell_t) -> Result<i32, Box<dyn Error>> {
    let this = MyExtension::closestpos_handle_type().read_handle(handleid, ctx.get_identity())?;
    let this = this.try_borrow()?;

    let pos = unsafe {
        std::mem::transmute::<*mut cell_t, &[f32; 3]>(
            ctx.local_to_phys_addr_ptr(pos_addr)?
        )
    };
    //println!("pos = {:?}", pos);

    /*
    let point = Point {
        data: 0,
        dims: *pos,
    };

    //let point = this.tree.nearest_search(&point);
    //Ok(point.data)

    // We have 8 total xmm registers that can be accessed "easily" in x32.
    // Going to number them innacurately.

    // xmm0
    let point = unsafe {
        _mm_load_ps(
            std::mem::transmute::<&Point, *const f32>(&point)
        )
    };

    // xmm1
    let mask = unsafe { _mm_set_ps(0xFFFFFFFFu32 as f32, 0xFFFFFFFFu32 as f32, 0xFFFFFFFFu32 as f32, 0.0) };

    let sums: &mut [Point; 4] = unsafe { std::mem::zeroed() };

    let mut closest_dist = f32::INFINITY;
    //let mut closest_dist = unsafe { _mm_set1_ps(closest_dist) };
    let mut closest_idx = -1i32;

    let count = this.points.len();
    let mut cur = this.points.as_ptr();
    let regs: &mut [__m128; 4] = unsafe { std::mem::uninitialized() };

    unsafe {
        for xxx in 0..(count/sums.len()) {
            for v in regs.iter_mut() {
                *v = _mm_load_ps(cur as *const f32);
                cur = cur.add(1);
            }

            for v in regs.iter_mut() {
                *v = _mm_sub_ps(point, *v);
                *v = _mm_mul_ps(*v, *v);
                *v = _mm_and_ps(*v, mask);
                *v = _mm_hadd_ps(*v, *v);
            }

            for (i, v) in sums.iter_mut().enumerate() {
                _mm_store_ps((*v).f.as_mut_ptr(), regs[i]);
            }

            for (i, v) in sums.iter().enumerate() {
                if v.f[0] < closest_dist {
                    closest_dist = v.f[0];
                    closest_idx = (xxx*sums.len()+i) as i32;
                }
            }
        }
    }

    if closest_idx > -1 {
        Ok(closest_idx + this.startidx)
    } else {
        Ok(-1)
    }
    */

    let (_dist, elem) = this.tree.nearest_one(pos, &squared_euclidean)?; // todo
    Ok(*elem)
}

#[derive(Default, SMExtension)]
#[extension(name = "ClosestPos-rs", description = "Provides a type that can be used to quickly find the closest point to the input position.")]
pub struct MyExtension {
    closestpos_handle_type: Option<HandleType<RefCell<ClosestPosContainer>>>,
    arraylist_handle_type: Option<HandleType<ICellArray>>,
}

impl MyExtension {
    /// Helper to get the extension singleton from the global provided by sm-ext.
    /// This is implemented here rather than by the SMExtension derive to aid code completion.
    fn get() -> &'static Self {
        EXTENSION_GLOBAL.with(|ext| unsafe { &(*ext.borrow().unwrap()).delegate })
    }

    fn closestpos_handle_type() -> &'static HandleType<RefCell<ClosestPosContainer>> {
        Self::get().closestpos_handle_type.as_ref().unwrap()
    }

    fn arraylist_handle_type() -> &'static HandleType<ICellArray> {
        Self::get().arraylist_handle_type.as_ref().unwrap()
    }
}

#[repr(C)]
#[allow(non_snake_case)]
struct QHandleType {
    dispatch: *const c_void,
    freeID: u32,
    children: u32,
    typeSec: TypeAccess,
}

#[repr(C)]
#[allow(non_snake_case)]
struct HandleSystem {
    vtable: *const c_void,
    m_Handles: *const c_void,
    m_Types: *const QHandleType,
}

impl IExtensionInterface for MyExtension {
    fn on_extension_load(&mut self, myself: IExtension, sys: IShareSys, late: bool) -> Result<(), Box<dyn std::error::Error>> {
        println!(">>> Rusty extension loaded! me = {:?}, sys = {:?}, late = {:?}", myself, sys, late);

        let handlesys: IHandleSys = sys.request_interface(&myself)?;

        let blah = unsafe { std::mem::transmute::<IHandleSys, *const HandleSystem>(handlesys) };
        #[allow(non_snake_case)]
        let g_pCoreIdent: IdentityTokenPtr = unsafe { (*(*blah).m_Types.offset(512)).typeSec.ident }; // still no idea why 512...
        let arraylist_id = handlesys.find_type("CellArray").ok_or("Couldn't find the CellArray (ArrayList) type.")?;
        self.arraylist_handle_type = Some(handlesys.faux_type(arraylist_id, g_pCoreIdent)?);

        self.closestpos_handle_type = Some(handlesys.create_type("ClosestPos", None, myself.get_identity())?);

        register_natives!(
            &sys,
            &myself,
            [
                ("ClosestPos.ClosestPos", native_closestpos_create),
                ("ClosestPos.Find", native_closestpos_find),
            ]
        );

        sys.register_library(&myself, "closestpos");

        Ok(())
    }

    fn on_extension_unload(&mut self) {
        self.closestpos_handle_type = None;
    }
}
