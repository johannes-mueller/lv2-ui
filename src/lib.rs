
extern crate lv2_sys;
extern crate lv2_core;
extern crate urid;

use lv2_sys as sys;

use std::os::raw::c_char;
use std::ffi::CStr;
use std::path::Path;
use std::str::Utf8Error;

use lv2_core::prelude::*;
use urid::Uri;
use std::fmt::Debug;

#[derive(Debug)]
pub enum PluginUIInfoError {
    InvalidBundlePathUtf8(Utf8Error),
}

pub struct UIPort<T: Default + Sized> {
    value: T,
    changed_by_ui: bool
}

pub struct UIPortRaw {
    data: *mut std::ffi::c_void,
    size: u32
}

impl<T: Copy + Debug + Default> UIPort<T> {
    pub fn new() -> Self {
	Self {
	    value: T::default(),
	    changed_by_ui: false,
	    //changed_by_port_event: false
	}
    }
    pub fn set_value(&mut self, v: T) {
	self.value = v;
	self.changed_by_ui = true;
    }
    pub fn value_as_ptr(&mut self) -> UIPortRaw {
	UIPortRaw {
		data: &mut self.value as *mut T as *mut std::ffi::c_void,
		size: std::mem::size_of::<T>() as u32
	}
    }
    pub fn value(&self) -> Option<T> {
	Some(self.value)
    }

}

pub trait UIPortsTrait : Sized {
    fn port_event(&mut self, port_index: u32, buffer_size: u32, format: u32, buffer: *const std::ffi::c_void) {
	let port_raw = match self.port_map(port_index) {
	    Some(pr) => pr,
	    None => {
		eprintln!("Unknown port index {}", port_index);
		return;
	    }
	};
	if buffer_size != port_raw.size {
	    eprintln!("Port buffer size mismatch. port_index: {}, expected {}, got {}",
		     port_index, port_raw.size, buffer_size);
	    return;
	}
	unsafe { std::ptr::copy_nonoverlapping(buffer, port_raw.data, buffer_size as usize); }
    }

    fn port_iterator(&mut self) -> UIPortIterator<Self> {
	UIPortIterator::<Self> { current_index: 0, ports: self }
    }

    fn port_map(&mut self, port_index: u32) -> Option<UIPortRaw>;
}

pub struct UIPortIterator<'a, T: UIPortsTrait> {
    ports: &'a mut T,
    current_index: u32
}

impl<'a, T: UIPortsTrait> Iterator for UIPortIterator<'a, T>  {
    type Item = UIPortRaw;
    fn next(&mut self) -> Option<Self::Item> {
	let ret = self.ports.port_map(self.current_index);
	self.current_index = match ret {
	    Some(_) => self.current_index + 1,
	    None => 0
	};
	ret
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

    fn new(plugin_ui_info: &PluginUIInfo, features: &mut Self::InitFeatures, parent_window: *mut std::ffi::c_void) -> Option<Self>;

    fn cleanup(&mut self);

    fn ports(&mut self) -> &mut Self::UIPorts;

    fn update(&mut self);

    fn port_event(&mut self, port_index: u32, buffer_size: u32, format: u32, buffer: *const std::ffi::c_void) {
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
		let handle = Box::new(Self {
		    instance,
		    write_function,
		    controller,
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
	eprintln!("unsafe idle {:?} {:?} {}", handle, &(*handle).instance as *const T as *const std::ffi::c_void, r);

	if let Some(func) = (*handle).write_function {
	    for (index, port_raw) in (*handle).instance.ports().port_iterator().enumerate() {
		func((*handle).controller, index as u32, port_raw.size, 0, port_raw.data);
	    }
	}
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
