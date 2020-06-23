
extern crate lv2_sys;
extern crate lv2_core;
extern crate lv2_atom;
extern crate urid;

use lv2_sys as sys;
use lv2_atom as atom;

use std::os::raw::c_char;
use std::ffi::CStr;
use std::path::Path;
use std::str::Utf8Error;
use std::ptr;

use lv2_core::prelude::*;
use atom::prelude::*;
use atom::space::RootMutSpace;
use urid::*;
use std::fmt::Debug;


#[derive(Debug)]
pub enum PluginUIInfoError {
    InvalidBundlePathUtf8(Utf8Error),
}

pub trait UIPort {
    fn index(&self) -> u32;

    fn protocol(&self) -> u32;

    fn size(&self) -> usize;

    fn data(&self) -> *const std::ffi::c_void;
}

pub struct UIControlPort {
    value: f32,
    changed: bool,
    index: u32
}

impl UIControlPort {
    pub fn new(index: u32) -> Self {
        UIControlPort {
            value: 0.0,
            changed: false,
            index
        }
    }
    pub fn set_value(&mut self, v: f32) {
        self.value = v;
        self.changed = true;
    }

    pub fn changed(&self) -> bool {
        self.changed
    }

    pub fn changed_value(&mut self) -> Option<f32> {
        match self.changed {
            false => None,
            true => {
                self.changed = false;
                Some(self.value)
            }
        }
    }
}

impl UIPort for UIControlPort {
    fn index(&self) -> u32 {
        self.index
    }
    fn protocol(&self) -> u32 {
        0
    }
    fn size(&self) -> usize {
        std::mem::size_of::<f32>()
    }
    fn data(&self) -> *const std::ffi::c_void {
        &self.value as *const f32 as *const std::ffi::c_void
    }
}

pub struct UIAtomPort {
    space_to_plugin: SelfAllocatingSpace,
    space_to_ui: SelfAllocatingSpace,
    urid: URID<atom::uris::EventTransfer>,
    index: u32,
}

impl UIAtomPort {
    pub fn new(urid: URID<atom::uris::EventTransfer>, index: u32) -> UIAtomPort {
        UIAtomPort {
            space_to_plugin: SelfAllocatingSpace::new(),
            space_to_ui: SelfAllocatingSpace::new(),
            urid,
            index,
        }
    }

    pub fn read<'a, A: atom::Atom<'a, 'a>>(
        &'a mut self,
        urid: URID<A>,
        parameter: A::ReadParameter
    ) -> Option<A::ReadHandle> {
        A::read(self.space_to_ui.take()?.split_atom_body(urid)?.0, parameter)
    }

    pub fn init<'a, A: atom::Atom<'a, 'a>>(
        &'a mut self,
        urid: URID<A>,
        parameter: A::WriteParameter
    ) -> Option<A::WriteHandle> {
        self.space_to_plugin = SelfAllocatingSpace::new();
        (&mut self.space_to_plugin as &mut dyn MutSpace).init(urid, parameter)
    }

    unsafe fn put_buffer(&mut self, buffer: std::ptr::NonNull<std::ffi::c_void>, size: usize) {
        self.space_to_ui.put_buffer(buffer, size);
    }
}

impl UIPort for UIAtomPort {
    fn index(&self) -> u32 {
        self.index
    }
    fn protocol(&self) -> u32 {
        self.urid.get()
    }
    fn size(&self) -> usize {
        self.space_to_plugin.data.len()
    }
    fn data(&self) -> *const std::ffi::c_void {
        self.space_to_plugin.data.as_ptr() as *const std::ffi::c_void
    }
}


struct SelfAllocatingSpace {
    data: Vec<u8>,
    already_read: bool
}

impl SelfAllocatingSpace {
    fn new() -> Self {
        SelfAllocatingSpace {
            data: Vec::new(),
            already_read: false,
        }
    }

    unsafe fn put_buffer(&mut self, buffer: std::ptr::NonNull<std::ffi::c_void>, size: usize) {
        self.data.set_len(0);
        self.data.reserve(size);
        std::ptr::copy_nonoverlapping(buffer.cast().as_ptr() as *const u8,
                                      self.data.as_mut_ptr(),
                                      size);
        self.data.set_len(size);
        self.already_read = false;
    }

    fn take(&mut self) -> Option<atom::space::Space> {
        if self.data.len() == 0 || self.already_read {
            return None
        }
        let space = atom::space::Space::from_slice(&self.data);
        self.already_read = true;
        Some(space)
    }
}

impl<'a> MutSpace<'a> for SelfAllocatingSpace {
    fn allocate(&mut self, size: usize, apply_padding: bool) -> Option<(usize, &'a mut [u8])> {
        let start_point = self.data.len();
        self.data.resize(start_point + size, 0);
        let return_slice = &mut self.data[start_point..];
        Some((0,
              unsafe {
                  std::slice::from_raw_parts_mut(return_slice.as_mut_ptr(), size)
        }))
    }
}


pub trait UIPortsTrait : Sized {
    fn port_event(&mut self, port_index: u32, buffer_size: u32, format: u32, buffer: *const std::ffi::c_void) {
        match format {
            0 => {
                let value: f32 = unsafe { *(buffer as *const f32) };
                match self.map_control_port(port_index) {
                    Some(ref mut port) => port.set_value(value),
                    None => eprintln!("unknown control port: {}", port_index)
                }
            }
            urid => {
                match self.map_atom_port(port_index) {
                    Some(ref mut port) => {
                        if port.urid.get() == urid {
                            if let Some(pointer) = ptr::NonNull::new(buffer as *mut std::ffi::c_void) {
                                unsafe {
                                    port.put_buffer(pointer, buffer_size as usize);
                                }
                            }
                        } else {
                            eprintln!("urids of port {} don't match", port_index);
                        }

                    }
                    None => eprintln!("unknown atom port: {}", port_index)
                }
            }
        }
    }

    fn map_control_port(&mut self, port_index: u32) -> Option<&mut UIControlPort>;

    fn map_atom_port(&mut self, port_index: u32) -> Option<&mut UIAtomPort>;
}


pub struct PluginPortWriteHandle {
    write_function: sys::LV2UI_Write_Function,
    controller: sys::LV2UI_Controller,
}

impl PluginPortWriteHandle {
    pub fn write_port(&self, port: &impl UIPort) {
        if let Some(write_function) = self.write_function {
            unsafe {
                write_function(self.controller,
                               port.index(),
                               port.size() as u32,
                               port.protocol(),
                               port.data()
                );
            }
        }
    }
}

pub struct PluginUIInfo<'a> {
    plugin_uri: &'a Uri,
    ui_uri: &'a Uri,
    bundle_path: &'a Path,
}

impl<'a> PluginUIInfo<'a> {
    pub unsafe fn from_raw(
        descriptor: *const sys::LV2UI_Descriptor,
        plugin_uri: *const c_char,
        bundle_path: *const c_char,
    ) -> Result<Self, PluginUIInfoError> {
        let bundle_path = Path::new(
            Uri::from_ptr(bundle_path)
                .to_str()
                .map_err(PluginUIInfoError::InvalidBundlePathUtf8)?,
        );
        Ok(Self::new(
            Uri::from_ptr(plugin_uri),
            Uri::from_ptr((*descriptor).URI),
            bundle_path,
        ))
    }

    pub fn new(plugin_uri: &'a Uri, ui_uri: &'a Uri, bundle_path: &'a Path) -> Self {
        Self {
            plugin_uri,
            ui_uri,
            bundle_path
        }
    }

    /// The URI of the plugin that is being instantiated.
    pub fn plugin_uri(&self) -> &Uri {
        self.plugin_uri
    }

    /// The URI of the UI that is being instantiated.
    pub fn ui_uri(&self) -> &Uri {
        self.ui_uri
    }

    /// The path to the LV2 bundle directory which contains this plugin binary.
    ///
    /// This is useful to get if the plugin needs to store extra resources in its bundle directory,
    /// such as presets, or any other kind of data.
    pub fn bundle_path(&self) -> &Path {
        self.bundle_path
    }
}


pub trait PluginUI : Sized + 'static {

    type InitFeatures: FeatureCollection<'static>;
    type UIPorts: UIPortsTrait;

    fn new(plugin_ui_info: &PluginUIInfo,
           features: &mut Self::InitFeatures,
           parent_window: *mut std::ffi::c_void,
           write_handle: PluginPortWriteHandle
    ) -> Option<Self>;

    fn cleanup(&mut self);

    fn ports(&mut self) -> &mut Self::UIPorts;

    fn update(&mut self);

    fn port_event(&mut self, port_index: u32, buffer_size: u32, format: u32, buffer: *const std::ffi::c_void) {
        //eprintln!("port_event: {}, {}", port_index, unsafe {*(buffer as *const f32)});
        self.ports().port_event(port_index, buffer_size, format, buffer);
        self.update();
    }

    fn widget(&self) -> sys::LV2UI_Widget;

    fn idle(&mut self) -> i32;
}

#[repr(C)]
pub struct PluginUIInstance<T: PluginUI> {
    instance: T,
    widget: sys::LV2UI_Widget,
    features: *const *const sys::LV2_Feature
}


fn retrieve_parent_window(features: *const *const sys::LV2_Feature) -> *mut std::ffi::c_void {
    let mut fptr = features;

    while !fptr.is_null() {
        unsafe {
            if CStr::from_ptr((**fptr).URI) == CStr::from_bytes_with_nul_unchecked(sys::LV2_UI__parent) {
                return (**fptr).data;
            }
            fptr = fptr.add(1);
        }
    }
    std::ptr::null_mut()
}

impl<T: PluginUI> PluginUIInstance<T> {

    pub unsafe extern "C" fn instantiate(
        descriptor: *const sys::LV2UI_Descriptor,
        plugin_uri: *const c_char,
        bundle_path: *const c_char,
        write_function: sys::LV2UI_Write_Function,
        controller: sys::LV2UI_Controller,
        widget: *mut sys::LV2UI_Widget,
        features: *const *const sys::LV2_Feature,
    ) -> sys::LV2UI_Handle {
        let descriptor = match descriptor.as_ref() {
            Some(descriptor) => descriptor,
            None => {
                eprintln!("Failed to initialize plugin UI: Descriptor points to null");
                return std::ptr::null_mut();
            }
        };

        let plugin_ui_info = match PluginUIInfo::from_raw(descriptor, plugin_uri, bundle_path) {
            Ok(info) => info,
            Err(e) => {
                eprintln!(
                    "Failed to initialize plugin: Illegal info from host: {:?}",
                    e
                );
                return std::ptr::null_mut();
            }
        };


        let mut feature_cache = FeatureCache::from_raw(features);

        let parent_widget = retrieve_parent_window(features);

        let mut init_features =
            match T::InitFeatures::from_cache(&mut feature_cache, ThreadingClass::Instantiation) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("extension data {}", e);
                    return std::ptr::null_mut();
                }
            };

        let write_handle = PluginPortWriteHandle {
            write_function,
            controller
        };

        match T::new(&plugin_ui_info, &mut init_features, parent_widget, write_handle) {
            Some(instance) => {
                *widget = instance.widget();
                let handle = Box::new(Self {
                    instance,
                    widget: *widget,
                    features
                });
                Box::leak(handle) as *mut Self as sys::LV2UI_Handle
            }
            None => std::ptr::null_mut()
        }
    }

    pub unsafe extern "C" fn cleanup(handle: sys::LV2UI_Handle) {
        let handle = handle as *mut Self;
        (*handle).instance.cleanup();
    }

    pub unsafe extern "C" fn port_event(handle: sys::LV2UI_Handle,
                                        port_index: u32,
                                        buffer_size: u32,
                                        format: u32,
                                        buffer: *const std::ffi::c_void) {
        let handle = handle as *mut Self;
        (*handle).instance.port_event(port_index, buffer_size, format, buffer);
    }

    pub unsafe extern "C" fn extension_data(uri: *const c_char) -> *const std::ffi::c_void {
        if CStr::from_ptr(uri) == CStr::from_bytes_with_nul_unchecked(sys::LV2_UI__idleInterface) {
            let interface = Box::new(sys::LV2UI_Idle_Interface { idle: Some(Self::idle) });
            Box::leak(interface) as *mut sys::LV2UI_Idle_Interface as *const std::ffi::c_void
        } else {
            std::ptr::null()
        }
    }

    pub unsafe extern "C" fn idle(handle: sys::LV2UI_Handle) -> i32 {
        let handle = handle as *mut Self;
        let r = (*handle).instance.idle();
        r
    }
}

pub unsafe trait PluginUIInstanceDescriptor {
    const DESCRIPTOR: sys::LV2UI_Descriptor;
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
