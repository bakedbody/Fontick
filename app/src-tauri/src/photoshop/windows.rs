#[derive(Debug, Clone)]
pub struct PlatformClient {
    app: windows::Win32::System::Com::IDispatch,
}

impl PlatformClient {
    pub fn active(expected_path: Option<&str>) -> Result<Self, String> {
        let app = get_active_photoshop()?;
        let ps = Self { app };
        if let Some(expected) = expected_path {
            if !expected.trim().is_empty() {
                let actual = ps.get_string_property("Path")?;
                if normalize_path(&actual) != normalize_path(expected) {
                    return Err(format!("Connected to wrong Photoshop: {}", actual));
                }
            }
        }
        Ok(ps)
    }

    pub fn path(&self) -> Result<String, String> {
        self.get_string_property("Path")
    }

    pub fn version(&self) -> Result<String, String> {
        self.get_string_property("Version")
    }

    pub fn do_javascript(&self, jsx: &str) -> Result<String, String> {
        unsafe {
            let dispid = get_dispid(&self.app, "DoJavaScript")?;
            let mut arg = variant_bstr(jsx);
            let mut params = windows::Win32::System::Com::DISPPARAMS {
                rgvarg: &mut arg,
                rgdispidNamedArgs: std::ptr::null_mut(),
                cArgs: 1,
                cNamedArgs: 0,
            };
            let mut result = windows::Win32::System::Variant::VARIANT::default();
            self.app
                .Invoke(
                    dispid,
                    &windows::core::GUID::zeroed(),
                    0x800,
                    windows::Win32::System::Com::DISPATCH_METHOD,
                    &mut params,
                    Some(&mut result),
                    None,
                    None,
                )
                .map_err(|err| format!("{err:?}"))?;
            variant_to_string(&mut result)
        }
    }

    fn get_string_property(&self, name: &str) -> Result<String, String> {
        unsafe {
            let dispid = get_dispid(&self.app, name)?;
            let mut result = windows::Win32::System::Variant::VARIANT::default();
            let mut params = windows::Win32::System::Com::DISPPARAMS::default();
            self.app
                .Invoke(
                    dispid,
                    &windows::core::GUID::zeroed(),
                    0x800,
                    windows::Win32::System::Com::DISPATCH_PROPERTYGET,
                    &mut params,
                    Some(&mut result),
                    None,
                    None,
                )
                .map_err(|err| format!("{err:?}"))?;
            variant_to_string(&mut result)
        }
    }
}

fn normalize_path(path: &str) -> String {
    path.trim().trim_end_matches(['\\', '/']).to_lowercase()
}

fn get_active_photoshop() -> Result<windows::Win32::System::Com::IDispatch, String> {
    use windows::core::Interface;
    use windows::Win32::System::Com::{
        CLSIDFromProgID, CoInitializeEx, IDispatch, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::System::Ole::GetActiveObject;

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let clsid = CLSIDFromProgID(windows::core::w!("Photoshop.Application"))
            .map_err(|err| format!("CLSIDFromProgID failed: {err:?}"))?;
        let mut unknown = None;
        GetActiveObject(&clsid, None, &mut unknown)
            .map_err(|err| format!("Photoshop is not running or COM is busy: {err:?}"))?;
        let unknown = unknown.ok_or_else(|| "GetActiveObject returned null".to_string())?;
        unknown
            .cast::<IDispatch>()
            .map_err(|err| format!("Photoshop COM object is not IDispatch: {err:?}"))
    }
}

unsafe fn get_dispid(
    dispatch: &windows::Win32::System::Com::IDispatch,
    name: &str,
) -> Result<i32, String> {
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let name_ptr = windows::core::PCWSTR(wide.as_ptr());
    let mut dispid = 0i32;
    dispatch
        .GetIDsOfNames(
            &windows::core::GUID::zeroed(),
            &name_ptr,
            1,
            0x800,
            &mut dispid,
        )
        .map_err(|err| format!("GetIDsOfNames({name}) failed: {err:?}"))?;
    Ok(dispid)
}

unsafe fn variant_bstr(text: &str) -> windows::Win32::System::Variant::VARIANT {
    use std::mem::ManuallyDrop;
    use windows::Win32::System::Variant::{VARIANT, VT_BSTR};

    let bstr = windows::core::BSTR::from(text);
    let mut value = VARIANT::default();
    (*value.Anonymous.Anonymous).vt = VT_BSTR;
    (*value.Anonymous.Anonymous).Anonymous.bstrVal = ManuallyDrop::new(bstr);
    value
}

unsafe fn variant_to_string(
    value: &mut windows::Win32::System::Variant::VARIANT,
) -> Result<String, String> {
    use windows::Win32::System::Variant::{
        VariantChangeType, VariantClear, VARIANT, VAR_CHANGE_FLAGS, VT_BSTR,
    };

    let mut converted = VARIANT::default();
    VariantChangeType(&mut converted, value, VAR_CHANGE_FLAGS(0), VT_BSTR)
        .map_err(|err| format!("VariantChangeType failed: {err:?}"))?;

    let bstr = &(*converted.Anonymous.Anonymous).Anonymous.bstrVal;
    let text = String::from_utf16_lossy(bstr);

    let _ = VariantClear(&mut converted);
    let _ = VariantClear(value);
    Ok(text)
}
