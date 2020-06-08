
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


#[uri("http://lv2plug.in/ns/ext/atom#eventTransfer")]
pub struct AtomEventTransfer;

#[derive(Debug)]
pub enum PluginUIInfoError {
    InvalidBundlePathUtf8(Utf8Error),
}

pub struct ControlPort {
    value: f32,
    changed: bool
}

impl ControlPort {
    pub fn new() -> Self {
        ControlPort {
            value: 0.0,
            changed: false
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

pub struct UIAtomPort {
    message: Option<Vec<u8>>,
    urid: URID<AtomEventTransfer>
}

impl UIAtomPort {
    pub fn new(urid: URID<AtomEventTransfer>) -> UIAtomPort {
        UIAtomPort {
            message: None,
            urid
        }
    }

    pub fn set_message(&mut self, msg: Vec<u8>) {
        self.message = Some(msg);
    }

    pub fn take_message(&mut self) -> Option<Vec<u8>> {
        self.message.take()
    }

    pub fn init<'a, A: atom::Atom<'a, 'a>>(
        &'a self,
        space: &'a mut RootMutSpace<'a>,
        urid: URID<A>,
        parameter: A::WriteParameter
    ) -> Option<A::WriteHandle> {
        //let mut space = &mut atom::space::RootMutSpace::new(&mut self.buffer);
        (space as &mut dyn MutSpace).init(urid, parameter)
    }
}



pub trait UIPortsTrait : Sized {
    fn port_event(&mut self, port_index: u32, buffer_size: u32, format: u32, buffer: *const std::ffi::c_void) {
        match format {
            0 => {
                let value: f32 = unsafe { *(buffer as *const f32) };
                match self.map_control_port(port_index) {
                    Some(ref mut port) => port.set_value(value),
                    None => println!("unknown control port: {}", port_index)
                }
            }
            urid => {
                match self.map_atom_port(port_index) {
                    Some(ref mut port) => {
                        if port.urid.get() == urid {
                            println!("matching port event {}", buffer_size);
                            let mut message: Vec<u8> = Vec::with_capacity(buffer_size as usize);
                            // FIXME: should work without copying
                            unsafe {
                                ptr::copy_nonoverlapping(buffer as *const u8,
                                                         message.as_mut_ptr(),
                                                         buffer_size as usize);
                                message.set_len(buffer_size as usize);
                            }
                            port.message = Some(message);
                        } else {
                            println!("urids of port {} don't match", port_index);
                        }

                    }
                    None => println!("unknown atom port: {}", port_index)
                }
            }
        }
    }

    fn map_control_port(&mut self, port_index: u32) -> Option<&mut ControlPort>;

    fn map_atom_port(&mut self, port_index: u32) -> Option<&mut UIAtomPort>;
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

    fn new(plugin_ui_info: &PluginUIInfo, features: &mut Self::InitFeatures, parent_window: *mut std::ffi::c_void) -> Option<Self>;

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
    write_function: sys::LV2UI_Write_Function,
    controller: sys::LV2UI_Controller,
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
                    eprintln!("{}", e);
                    return std::ptr::null_mut();
                }
            };

        match T::new(&plugin_ui_info, &mut init_features, parent_widget) {
            Some(instance) => {
                *widget = instance.widget();
                let mut handle = Box::new(Self {
                    instance,
                    write_function,
                    controller,
                    widget: *widget,
                    features
                });
                Self::write_ports(&mut handle);
                Box::leak(handle) as *mut Self as sys::LV2UI_Handle
            }
            None => std::ptr::null_mut()
        }
    }

    pub unsafe extern "C" fn cleanup(handle: sys::LV2UI_Handle) {
        let handle = handle as *mut Self;
        (*handle).instance.cleanup();
        Self::write_ports(&mut (*handle));
    }

    pub unsafe extern "C" fn port_event(handle: sys::LV2UI_Handle, port_index: u32, buffer_size: u32, format: u32, buffer: *const std::ffi::c_void) {
        let handle = handle as *mut Self;
        (*handle).instance.port_event(port_index, buffer_size, format, buffer);
    }

    pub unsafe extern "C" fn extension_data(uri: *const c_char) -> *const std::ffi::c_void {
        eprintln!("extension_data {:?}", CStr::from_ptr(uri));
        if CStr::from_ptr(uri) == CStr::from_bytes_with_nul_unchecked(sys::LV2_UI__idleInterface) {
            let interface = Box::new(sys::LV2UI_Idle_Interface { idle: Some(Self::idle) });
            Box::leak(interface) as *mut sys::LV2UI_Idle_Interface as *const std::ffi::c_void
        } else if
            CStr::from_ptr(uri) == CStr::from_bytes_with_nul_unchecked(sys::LV2_UI__showInterface) {
                let interface = Box::new(sys::LV2UI_Idle_Interface { idle: Some(Self::idle) });
                Box::leak(interface) as *mut sys::LV2UI_Idle_Interface as *const std::ffi::c_void
        } else {
            std::ptr::null()
        }
    }

    pub unsafe extern "C" fn idle(handle: sys::LV2UI_Handle) -> i32 {
        let handle = handle as *mut Self;
        let r = (*handle).instance.idle();
        //eprintln!("unsafe idle {:?} {:?} {}", handle, &(*handle).instance as *const T as *const std::ffi::c_void, r);

        Self::write_ports(&mut (*handle));
        r
    }

    fn write_ports(handle: &mut Self) {
        if let Some(func) = handle.write_function {
            let mut index = 0;
            loop {
                if let Some(ref port) = handle.instance.ports().map_control_port(index) {
                    if port.changed() {
                        unsafe {
                            func(handle.controller,
                                 index,
                                 std::mem::size_of::<f32>() as u32,
                                 0,
                                 &port.value as *const f32 as *const std::ffi::c_void);
                        }
                    }
                } else if let Some(ref mut port) = handle.instance.ports().map_atom_port(index) {
                    if let Some(msg) = port.message.take() {
                        unsafe {
                            func(handle.controller,
                                 index,
                                 msg.len() as u32,
                                 port.urid.get(),
                                 msg.as_slice() as *const [u8] as *const std::ffi::c_void);
                        }
                    }
                } else {
                    break;
                }
                index += 1;
            }
        }

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
