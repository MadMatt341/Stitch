use std::{
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};
use windows::{
    Win32::{
        Foundation::{ERROR_CANCELLED, HWND},
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
            CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
        },
        UI::Shell::{
            Common::COMDLG_FILTERSPEC, FOS_ALLOWMULTISELECT, FOS_FILEMUSTEXIST,
            FOS_FORCEFILESYSTEM, FOS_NOCHANGEDIR, FOS_PATHMUSTEXIST, FileOpenDialog,
            IFileOpenDialog, IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
        },
    },
    core::{HRESULT, PCWSTR, Result, w},
};

struct Apartment;

impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

pub unsafe fn open(
    owner: windows_sys::Win32::Foundation::HWND,
    folder: Option<&Path>,
) -> Result<Vec<PathBuf>> {
    CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).ok()?;
    let _apartment = Apartment;
    let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
    dialog.SetOptions(
        dialog.GetOptions()?
            | FOS_ALLOWMULTISELECT
            | FOS_FORCEFILESYSTEM
            | FOS_FILEMUSTEXIST
            | FOS_PATHMUSTEXIST
            | FOS_NOCHANGEDIR,
    )?;
    dialog.SetTitle(w!("Add videos"))?;
    if let Some(folder) = folder {
        let name: Vec<_> = folder.as_os_str().encode_wide().chain(Some(0)).collect();
        let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(name.as_ptr()), None)?;
        dialog.SetFolder(&item)?;
    }
    dialog.SetFileTypes(&[
        COMDLG_FILTERSPEC {
            pszName: w!("Video files"),
            pszSpec: w!("*.mp4;*.mkv;*.mov;*.m4v;*.webm;*.avi;*.ts;*.mts;*.m2ts;*.wmv"),
        },
        COMDLG_FILTERSPEC {
            pszName: w!("All files"),
            pszSpec: w!("*.*"),
        },
    ])?;
    if let Err(error) = dialog.Show(Some(HWND(owner))) {
        if error.code() == HRESULT::from_win32(ERROR_CANCELLED.0) {
            return Ok(Vec::new());
        }
        return Err(error);
    }
    let items = dialog.GetResults()?;
    let mut paths = Vec::new();
    for index in 0..items.GetCount()? {
        let name = items.GetItemAt(index)?.GetDisplayName(SIGDN_FILESYSPATH)?;
        paths.push(PathBuf::from(std::ffi::OsString::from_wide(name.as_wide())));
        CoTaskMemFree(Some(name.0 as _));
    }
    Ok(paths)
}
