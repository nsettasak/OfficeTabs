use crate::placement::*;
use crate::strip::Strip;
use crate::{
    automation::{Dispatch, Value},
    model::*,
};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{LibraryLoader::*, Threading::GetCurrentProcessId},
    UI::{HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};
type SubProc =
    Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM, usize, usize) -> LRESULT>;
#[link(name = "comctl32")]
extern "system" {
    fn SetWindowSubclass(h: HWND, p: SubProc, id: usize, data: usize) -> i32;
    fn RemoveWindowSubclass(h: HWND, p: SubProc, id: usize) -> i32;
    fn DefSubclassProc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT;
}
pub static HOST_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
const STOP: u32 = WM_APP + 103;
const COMMAND: u32 = WM_APP + 101;
const DIRTY: u32 = WM_APP + 102;
/// A snapshot is forced this often even when the windows look unchanged: Excel
/// keeps no asterisk in the title, so saving it changes no window state.
const FULL_REFRESH_SECONDS: u64 = 5;
thread_local! {static HOSTS:RefCell<HashMap<usize,Runtime>>=RefCell::new(HashMap::new());}
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
pub(crate) fn hwnd(id: usize) -> HWND {
    id as HWND
}
pub(crate) fn host_name(excel: bool) -> &'static str {
    if excel {
        "Excel"
    } else {
        "Word"
    }
}
pub(crate) fn settings(excel: bool, key: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default())
        .join("OfficeTabs")
        .join("Settings")
        .join(format!("{key}-{}.txt", host_name(excel)))
}
pub fn enabled(excel: bool) -> bool {
    std::fs::read_to_string(settings(excel, "enabled"))
        .unwrap_or_default()
        .trim()
        != "disabled"
}
pub fn setting(excel: bool, key: &str, value: &str) -> std::io::Result<()> {
    let p = settings(excel, key);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(p, value)
}
fn log(text: &str) {
    if let Some(root) = std::env::var_os("LOCALAPPDATA") {
        let dir = std::path::PathBuf::from(root)
            .join("OfficeTabs")
            .join("Logs");
        let _ = std::fs::create_dir_all(&dir);
        use std::io::Write;
        let path = dir.join("Rust-native.log");
        // Office keeps this DLL loaded for the whole session; cap the file so a
        // long-running session cannot grow the log without bound.
        const LIMIT: u64 = 512 * 1024;
        if std::fs::metadata(&path)
            .map(|m| m.len() > LIMIT)
            .unwrap_or(false)
        {
            let _ = std::fs::rename(&path, dir.join("Rust-native.old.log"));
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = writeln!(f, "{:?} {}", std::time::SystemTime::now(), text);
        }
    }
}

/// Records a panic that `guard` contained, so a tab glitch can be diagnosed
/// without the user losing a document to a process abort.
pub fn record_panic(what: &str, detail: &str) {
    log(&format!("contained panic in {what}: {detail}"));
}
#[derive(Clone)]
enum Command {
    Activate(usize),
    Close(usize),
    New,
    Drop(DropAction),
    CloseAll(usize),
}
struct Runtime {
    controller: usize,
    app: Dispatch,
    excel: bool,
    model: Model,
    objects: HashMap<usize, Dispatch>,
    strips: HashMap<usize, Strip>,
    hidden: HashSet<usize>,
    queue: VecDeque<Command>,
    preview: usize,
    bypass: bool,
    closing: bool,
    /// Documents from the last COM snapshot, reused while the windows look unchanged.
    cached: Vec<Document>,
    /// Fingerprint of the Office windows this snapshot was taken from.
    state: u64,
    last_full: Instant,
    primed: bool,
    /// Last snapshot failure and when it was logged, so a busy host writes one
    /// line instead of one per second until it recovers.
    failure: Option<(i32, Instant)>,
}
pub(crate) unsafe fn rect(h: HWND) -> RECT {
    let mut r = zeroed();
    GetWindowRect(h, &mut r);
    r
}
pub(crate) unsafe fn client_rect(h: HWND) -> RECT {
    let mut r = zeroed();
    GetClientRect(h, &mut r);
    r
}
pub(crate) unsafe fn parent_rect(h: HWND, parent: HWND) -> RECT {
    let mut r = rect(h);
    MapWindowPoints(null_mut(), parent, &mut r as *mut _ as *mut POINT, 2);
    r
}
pub(crate) fn same(a: RECT, b: RECT) -> bool {
    a.left == b.left && a.top == b.top && a.right == b.right && a.bottom == b.bottom
}
unsafe fn cursor() -> POINT {
    let mut p = zeroed();
    GetCursorPos(&mut p);
    p
}
unsafe fn instance() -> HINSTANCE {
    let mut h = null_mut();
    GetModuleHandleExW(
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        controller_proc as *const u16,
        &mut h,
    );
    h
}
unsafe fn register() {
    let module = instance();
    for (name, proc) in [
        (
            "OfficeTabs.Rust.Controller",
            Some(controller_proc as unsafe extern "system" fn(_, _, _, _) -> _),
        ),
        (
            "OfficeTabs.Rust.Strip",
            Some(strip_proc as unsafe extern "system" fn(_, _, _, _) -> _),
        ),
    ] {
        let class = wide(name);
        let cls = WNDCLASSW {
            lpfnWndProc: proc,
            hInstance: module,
            lpszClassName: class.as_ptr(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            ..zeroed()
        };
        RegisterClassW(&cls);
    }
}
/// # Safety
/// `app` must be a live Office Application object; call it from the Office UI thread.
pub unsafe fn start(app: Dispatch, excel: bool) -> Result<usize, i32> {
    register();
    let h = CreateWindowExW(
        0,
        wide("OfficeTabs.Rust.Controller").as_ptr(),
        wide("OfficeTabs").as_ptr(),
        0,
        0,
        0,
        0,
        0,
        HWND_MESSAGE,
        null_mut(),
        instance(),
        null(),
    );
    if h.is_null() {
        return Err(0x80004005u32 as i32);
    }
    let runtime = Runtime {
        controller: h as usize,
        app,
        excel,
        model: Model::default(),
        objects: HashMap::new(),
        strips: HashMap::new(),
        hidden: HashSet::new(),
        queue: VecDeque::new(),
        preview: 0,
        bypass: false,
        closing: false,
        cached: Vec::new(),
        state: 0,
        last_full: Instant::now(),
        primed: false,
        failure: None,
    };
    HOSTS.with(|hosts| hosts.borrow_mut().insert(h as usize, runtime));
    HOST_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    SetTimer(h, 1, 1000, None);
    PostMessageW(h, DIRTY, 0, 0);
    log(&format!(
        "Native Rust {} connected to {}",
        env!("CARGO_PKG_VERSION"),
        host_name(excel)
    ));
    Ok(h as usize)
}
/// # Safety
/// `id` must be a controller handle returned by `start` (0 is ignored).
pub unsafe fn stop(id: usize) {
    if id == 0 {
        return;
    }
    let result = HOSTS.with(|hosts| match hosts.try_borrow_mut() {
        Ok(mut h) => Ok(h.remove(&id)),
        Err(_) => Err(()),
    });
    match result {
        Ok(Some(mut host)) => {
            host.cleanup();
            KillTimer(hwnd(id), 1);
            DestroyWindow(hwnd(id));
            HOST_COUNT.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
            if HOST_COUNT.load(std::sync::atomic::Ordering::Acquire) == 0 {
                UnregisterClassW(wide("OfficeTabs.Rust.Controller").as_ptr(), instance());
                UnregisterClassW(wide("OfficeTabs.Rust.Strip").as_ptr(), instance());
            }
        }
        Err(()) => {
            PostMessageW(hwnd(id), STOP, 0, 0);
        }
        _ => {}
    }
}
unsafe extern "system" fn controller_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // Containment: a panic in tab bookkeeping must not reach Office's loop.
    match crate::guard::window("controller", || unsafe { controller_body(h, m, w, l) }) {
        Some(value) => value,
        None => DefWindowProcW(h, m, w, l),
    }
}
unsafe fn controller_body(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if m == STOP {
        stop(h as usize);
        return 0;
    }
    if m == WM_TIMER || m == DIRTY || m == COMMAND {
        HOSTS.with(|hosts| {
            if let Ok(mut map) = hosts.try_borrow_mut() {
                if let Some(host) = map.get_mut(&(h as usize)) {
                    if m == COMMAND {
                        while let Some(cmd) = host.queue.pop_front() {
                            if let Err(hr) = host.execute(cmd) {
                                log(&format!("Office action failed/cancelled: {hr:08x}"));
                            }
                        }
                    }
                    if !host
                        .strips
                        .values()
                        .any(|s| s.drag.document.is_some() || s.moving)
                    {
                        if let Err(hr) = host.refresh() {
                            host.report_failure(hr);
                        }
                    }
                }
            }
        });
        return 0;
    }
    DefWindowProcW(h, m, w, l)
}
impl Runtime {
    unsafe fn enqueue(&mut self, c: Command) {
        if matches!(c, Command::CloseAll(_))
            && self
                .queue
                .iter()
                .any(|queued| matches!(queued, Command::CloseAll(_)))
        {
            return;
        }
        self.queue.push_back(c);
        PostMessageW(hwnd(self.controller), COMMAND, 0, 0);
    }
    unsafe fn snapshot(&self) -> Result<(Vec<Document>, HashMap<usize, Dispatch>), i32> {
        let collection = self
            .app
            .object(if self.excel { "Workbooks" } else { "Documents" })?;
        let count = collection.get("Count")?.number();
        let active = if count > 0 {
            self.app
                .object(if self.excel {
                    "ActiveWorkbook"
                } else {
                    "ActiveDocument"
                })
                .ok()
                .map(|d| d.identity())
        } else {
            None
        };
        let mut docs = Vec::new();
        let mut objects = HashMap::new();
        for i in 1..=count {
            let item = collection.item(i)?;
            if self.excel && item.get("IsAddin").map(|v| v.boolean()).unwrap_or(false) {
                continue;
            }
            let windows = item.object("Windows")?;
            if windows.get("Count")?.number() == 0 {
                continue;
            }
            let window = windows.item(1)?;
            let frame = GetAncestor(
                window.get("Hwnd")?.number() as u32 as usize as HWND,
                GA_ROOT,
            ) as usize;
            if frame == 0 {
                continue;
            }
            // Excel's start screen (and other full-page panes) replace the grid with a
            // FullpageUIHost child and hide XLDESK, while a Book1 workbook object still
            // exists. Such a window shows no document, so it gets no tabs. A window that
            // is hidden belongs to the group (its document is a tab), so it still counts.
            if !counts_as_document(frame, self.excel) {
                continue;
            }
            let id = item.identity();
            docs.push(Document {
                id,
                name: item.get("Name")?.string(),
                saved: item.get("Saved")?.boolean(),
                active: active == Some(id),
                frame,
            });
            objects.insert(id, item);
        }
        Ok((docs, objects))
    }
    unsafe fn refresh(&mut self) -> Result<(), i32> {
        if !enabled(self.excel) {
            self.clear_strips();
            return Ok(());
        }
        let fingerprint = self.fingerprint();
        if !self.primed
            || fingerprint != self.state
            || self.last_full.elapsed().as_secs() >= FULL_REFRESH_SECONDS
        {
            let (docs, objects) = self.snapshot()?;
            self.objects = objects;
            self.cached = docs;
            self.state = fingerprint;
            self.primed = true;
            self.last_full = Instant::now();
        }
        self.model.snapshot(self.cached.clone());
        self.model.prune_frames(|id| IsWindow(hwnd(id)) != 0);
        let frames: HashSet<usize> = self.model.documents.iter().map(|d| d.frame).collect();
        for frame in self.strips.keys().copied().collect::<Vec<_>>() {
            let invalid = self.strips.get(&frame).is_some_and(|s| {
                IsWindow(hwnd(s.window)) == 0
                    || IsWindow(hwnd(s.workspace)) == 0
                    || GetParent(hwnd(s.workspace)) != hwnd(s.parent)
            });
            if !frames.contains(&frame) || invalid {
                self.remove_strip(frame);
            }
        }
        if frames.is_empty() {
            self.restore_hidden();
            return Ok(());
        }
        for frame in frames {
            if !self.strips.contains_key(&frame) {
                self.attach(frame);
            }
            // Compare in place; only clone the view when the strip really changed.
            let stale = self
                .strips
                .get(&frame)
                .is_none_or(|strip| !strip.docs.iter().eq(self.model.view_iter(frame)));
            if stale {
                let view = self.model.view(frame);
                if let Some(strip) = self.strips.get_mut(&frame) {
                    strip.update(view);
                }
            }
        }
        self.group();
        // Office can synchronously relayout its workspace inside Activate/ShowWindow.
        // Those messages cannot borrow HOSTS while this controller owns it. Reconcile
        // after COM returns instead of assuming that the subclass saw every layout.
        for strip in self.strips.values_mut() {
            if IsWindowVisible(hwnd(strip.parent)) != 0 {
                strip.reconcile_workspace();
            }
        }
        Ok(())
    }
    unsafe fn group(&mut self) {
        let Some(active) = self
            .model
            .documents
            .iter()
            .find(|d| d.active)
            .map(|d| d.frame)
        else {
            return;
        };
        if self.model.independent.contains(&active) {
            return;
        }
        if !self.strips.contains_key(&active) {
            // Keep the current usable frame until Office exposes the target workspace.
            return;
        }
        if let Some(previous) = self.model.group_frame {
            if previous != active
                && IsWindow(hwnd(previous)) != 0
                && !self.model.independent.contains(&previous)
            {
                copy_placement(hwnd(previous), hwnd(active));
            }
        }
        if self.hidden.remove(&active) {
            ShowWindow(hwnd(active), SW_SHOWNA);
        }
        for frame in self.strips.keys() {
            if *frame != active
                && !self.model.independent.contains(frame)
                && IsWindowVisible(hwnd(*frame)) != 0
            {
                self.hidden.insert(*frame);
                ShowWindow(hwnd(*frame), SW_HIDE);
            }
        }
        self.model.group_frame = Some(active);
    }
    unsafe fn attach(&mut self, frame: usize) {
        let mut search = (if self.excel { "XLDESK" } else { "_WwB" }, 0usize);
        EnumChildWindows(
            hwnd(frame),
            Some(find_workspace),
            &mut search as *mut _ as isize,
        );
        if search.1 == 0 {
            return;
        }
        let workspace = hwnd(search.1);
        let parent = GetParent(workspace);
        let original = parent_rect(workspace, parent);
        let height = (44 * GetDpiForWindow(hwnd(frame)) / 96).max(36) as i32;
        if original.bottom - original.top < height + 100 {
            return;
        }
        let window = CreateWindowExW(
            0,
            wide("OfficeTabs.Rust.Strip").as_ptr(),
            wide("OfficeTabs document tabs").as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
            original.left,
            original.top,
            original.right - original.left,
            height,
            parent,
            null_mut(),
            instance(),
            null(),
        );
        if window.is_null() {
            return;
        }
        SetWindowLongPtrW(window, GWLP_USERDATA, self.controller as isize);
        let font = CreateFontW(
            -((14 * GetDpiForWindow(hwnd(frame)) / 96) as i32),
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            DEFAULT_PITCH as u32,
            wide("Segoe UI").as_ptr(),
        );
        let mut applied = original;
        applied.top += height;
        SetWindowPos(
            workspace,
            null_mut(),
            applied.left,
            applied.top,
            applied.right - applied.left,
            applied.bottom - applied.top,
            SWP_NOACTIVATE | SWP_NOZORDER,
        );
        self.strips.insert(
            frame,
            Strip {
                window: window as usize,
                workspace: workspace as usize,
                parent: parent as usize,
                original,
                applied,
                docs: vec![],
                rects: vec![],
                positions: vec![],
                labels: vec![],
                last: Instant::now(),
                drag: Drag::default(),
                offset: 0,
                marker: None,
                font,
                measures: HashMap::new(),
                moving: false,
                start: RECT::default(),
            },
        );
        SetWindowSubclass(hwnd(frame), Some(frame_proc), 0x544142, self.controller);
        SetWindowSubclass(workspace, Some(workspace_proc), 0x544143, self.controller);
    }
    unsafe fn remove_strip(&mut self, frame: usize) {
        if let Some(s) = self.strips.remove(&frame) {
            RemoveWindowSubclass(hwnd(frame), Some(frame_proc), 0x544142);
            RemoveWindowSubclass(hwnd(s.workspace), Some(workspace_proc), 0x544143);
            DestroyWindow(hwnd(s.window));
            DeleteObject(s.font);
            if IsWindow(hwnd(s.workspace)) != 0 {
                let r = parent_rect(hwnd(s.workspace), hwnd(s.parent));
                if r.top == s.applied.top {
                    SetWindowPos(
                        hwnd(s.workspace),
                        null_mut(),
                        r.left,
                        s.original.top,
                        r.right - r.left,
                        r.bottom - s.original.top,
                        SWP_NOACTIVATE | SWP_NOZORDER,
                    );
                }
            }
        }
    }
    unsafe fn restore_hidden(&mut self) {
        for frame in self.hidden.drain() {
            if IsWindow(hwnd(frame)) != 0 {
                ShowWindow(hwnd(frame), SW_SHOWNA);
            }
        }
        self.model.group_frame = None;
    }
    unsafe fn clear_strips(&mut self) {
        for f in self.strips.keys().copied().collect::<Vec<_>>() {
            self.remove_strip(f);
        }
        self.restore_hidden();
    }
    unsafe fn cleanup(&mut self) {
        self.clear_strips();
        if self.preview != 0 {
            DestroyWindow(hwnd(self.preview));
        }
    }
    unsafe fn target(&self, source: usize, p: POINT) -> Option<usize> {
        self.strips
            .iter()
            .filter(|(frame, _)| **frame != source && IsWindowVisible(hwnd(**frame)) != 0)
            .find_map(|(frame, s)| {
                let r = rect(hwnd(s.window));
                if p.x >= r.left - 16
                    && p.x <= r.right + 16
                    && p.y >= r.top - 20
                    && p.y <= r.bottom + 20
                {
                    Some(*frame)
                } else {
                    None
                }
            })
    }
    unsafe fn preview(&mut self, source: usize, p: POINT) {
        let target = self.target(source, p);
        let r = target
            .map(|f| rect(hwnd(f)))
            .unwrap_or_else(|| drop_rect(p));
        if self.preview == 0 {
            self.preview = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                wide("STATIC").as_ptr(),
                wide("").as_ptr(),
                WS_POPUP | 6,
                0,
                0,
                0,
                0,
                null_mut(),
                null_mut(),
                instance(),
                null(),
            ) as usize;
            SetLayeredWindowAttributes(hwnd(self.preview), 0, 110, LWA_ALPHA);
        }
        SetWindowPos(
            hwnd(self.preview),
            HWND_TOPMOST,
            r.left,
            r.top,
            r.right - r.left,
            r.bottom - r.top,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
    unsafe fn hide_preview(&self) {
        if self.preview != 0 {
            ShowWindow(hwnd(self.preview), SW_HIDE);
        }
    }
    unsafe fn execute(&mut self, c: Command) -> Result<(), i32> {
        match c {
            Command::Activate(id) => {
                let item = self.objects.get(&id).ok_or(0x80004005u32 as i32)?.clone();
                let frame = self
                    .model
                    .documents
                    .iter()
                    .find(|d| d.id == id)
                    .map(|d| d.frame)
                    .unwrap_or(0);
                let placement = if !self.model.independent.contains(&frame) {
                    self.model
                        .group_frame
                        .filter(|source| *source != frame)
                        .and_then(|source| capture_placement(hwnd(source)))
                } else {
                    None
                };
                if let Some(p) = placement {
                    apply_placement(hwnd(frame), p);
                }
                if self.hidden.remove(&frame) {
                    ShowWindow(hwnd(frame), SW_SHOWNA);
                }
                item.invoke("Activate")?;
                // Office may restore the target's previous bounds during Activate.
                if let Some(p) = placement {
                    apply_placement(hwnd(frame), p);
                }
            }
            Command::Close(id) => {
                let item = self.objects.get(&id).ok_or(0x80004005u32 as i32)?.clone();
                let alerts = if self.excel {
                    self.app.get("DisplayAlerts")?.boolean()
                } else {
                    true
                };
                self.bypass = true;
                let result = if self.excel {
                    if !alerts {
                        Err(0x80004005u32 as i32)
                    } else {
                        item.call("Close", 1, vec![Value::missing()]).map(|_| ())
                    }
                } else {
                    item.call("Close", 1, vec![Value::int(-2)]).map(|_| ())
                };
                self.bypass = false;
                result?;
            }
            Command::New => {
                self.app
                    .object(if self.excel { "Workbooks" } else { "Documents" })?
                    .invoke("Add")?;
            }
            Command::Drop(action) => match action {
                DropAction::Activate(id) => self.execute(Command::Activate(id))?,
                DropAction::Reorder(id, before) => self.model.reorder(id, before),
                DropAction::Detach(id, x, y) => {
                    let Some(doc) = self.model.documents.iter().find(|d| d.id == id).cloned()
                    else {
                        return Ok(());
                    };
                    if self
                        .model
                        .documents
                        .iter()
                        .any(|d| d.id != id && d.frame == doc.frame)
                    {
                        return Err(0x80004001u32 as i32);
                    }
                    self.model.detach(doc.frame);
                    self.hidden.remove(&doc.frame);
                    if let Some(other) = self.model.documents.iter().find(|d| {
                        d.frame != doc.frame && !self.model.independent.contains(&d.frame)
                    }) {
                        ShowWindow(hwnd(other.frame), SW_SHOWNA);
                        self.hidden.remove(&other.frame);
                        self.model.group_frame = Some(other.frame);
                    }
                    let r = drop_rect(POINT { x, y });
                    ShowWindow(hwnd(doc.frame), SW_RESTORE);
                    SetWindowPos(
                        hwnd(doc.frame),
                        null_mut(),
                        r.left,
                        r.top,
                        r.right - r.left,
                        r.bottom - r.top,
                        SWP_NOACTIVATE | SWP_NOZORDER,
                    );
                    self.objects[&id].invoke("Activate")?;
                }
                DropAction::Dock(id, target) => {
                    let Some(source) = self
                        .model
                        .documents
                        .iter()
                        .find(|d| d.id == id)
                        .map(|d| d.frame)
                    else {
                        return Ok(());
                    };
                    if source == target || IsWindow(hwnd(target)) == 0 {
                        return Ok(());
                    }
                    let placement = capture_placement(hwnd(target));
                    self.objects[&id].invoke("Activate")?;
                    if let Some(p) = placement {
                        apply_placement(hwnd(source), p);
                    }
                    self.model.dock(source, target);
                    self.model.group_frame = Some(source);
                }
                DropAction::Cancel => {}
            },
            Command::CloseAll(frame) => {
                if self.closing {
                    return Ok(());
                }
                self.closing = true;
                let choice = close_prompt(hwnd(frame), self.excel);
                self.closing = false;
                if choice == 100 && !self.model.independent.is_empty() {
                    let ids: Vec<usize> = self.model.view(frame).iter().map(|d| d.id).collect();
                    for id in ids {
                        self.execute(Command::Close(id))?;
                        let (docs, _) = self.snapshot()?;
                        if docs.iter().any(|d| d.id == id) {
                            break;
                        }
                    }
                } else if choice == 100 {
                    let alerts = if self.excel {
                        self.app.get("DisplayAlerts")?.boolean()
                    } else {
                        true
                    };
                    self.restore_hidden();
                    self.bypass = true;
                    let result = if self.excel {
                        if alerts {
                            self.app.invoke("Quit")
                        } else {
                            Err(0x80004005u32 as i32)
                        }
                    } else {
                        self.app.call("Quit", 1, vec![Value::int(-2)]).map(|_| ())
                    };
                    self.bypass = false;
                    result?;
                } else if choice == 101 {
                    if let Some(id) = self.model.documents.iter().find(|d| d.active).map(|d| d.id) {
                        self.execute(Command::Close(id))?;
                    }
                }
            }
        }
        Ok(())
    }
}
impl Runtime {
    /// Cheap change detector for the expensive COM snapshot: the Office frames
    /// with their titles and visibility, plus the foreground window. Renames,
    /// saves, activation, opening and closing all move this value.
    unsafe fn fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut frames: Vec<(usize, String, i32)> = Vec::new();
        EnumWindows(Some(collect_frames), &mut frames as *mut _ as isize);
        frames.sort_unstable_by_key(|frame| frame.0);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        frames.hash(&mut hasher);
        (GetForegroundWindow() as usize).hash(&mut hasher);
        hasher.finish()
    }
    /// Office reports a snapshot failure once; a host that stays busy (a modal
    /// dialog, a long save) must not write the same line every second.
    fn report_failure(&mut self, hr: i32) {
        let now = Instant::now();
        let repeated = self
            .failure
            .is_some_and(|(code, at)| code == hr && now.duration_since(at).as_secs() < 30);
        if !repeated {
            self.failure = Some((hr, now));
            log(&format!("Snapshot retry: {hr:08x}"));
        }
    }
}

/// Collects this process's Word and Excel top-level windows for `fingerprint`.
unsafe extern "system" fn collect_frames(h: HWND, l: LPARAM) -> i32 {
    let frames = &mut *(l as *mut Vec<(usize, String, i32)>);
    let mut class = [0u16; 64];
    let n = GetClassNameW(h, class.as_mut_ptr(), 64);
    let name = String::from_utf16_lossy(&class[..n as usize]);
    if name != "OpusApp" && name != "XLMAIN" {
        return 1;
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(h, &mut pid);
    if pid != GetCurrentProcessId() {
        return 1;
    }
    let mut title = [0u16; 260];
    let n = GetWindowTextW(h, title.as_mut_ptr(), 260);
    frames.push((
        h as usize,
        String::from_utf16_lossy(&title[..n as usize]),
        IsWindowVisible(h),
    ));
    1
}

/// True when `frame` should get tabs. A hidden frame belongs to the tab group
/// (its document is a tab), so only a visible frame has to prove it shows a
/// document area. Excel's start screen keeps a `Book1` workbook object alive
/// while a `FullpageUIHost` pane replaces the grid and `XLDESK` is hidden.
pub(crate) unsafe fn counts_as_document(frame: usize, excel: bool) -> bool {
    IsWindowVisible(hwnd(frame)) == 0 || workspace_visible(frame, excel)
}

/// True when the frame shows a document area (a visible XLDESK/_WwB workspace).
pub(crate) unsafe fn workspace_visible(frame: usize, excel: bool) -> bool {
    let mut search = (if excel { "XLDESK" } else { "_WwB" }, 0usize);
    EnumChildWindows(
        hwnd(frame),
        Some(find_workspace),
        &mut search as *mut _ as isize,
    );
    search.1 != 0
}
unsafe extern "system" fn find_workspace(h: HWND, l: LPARAM) -> i32 {
    let search = &mut *(l as *mut (&str, usize));
    let mut text = [0u16; 64];
    let n = GetClassNameW(h, text.as_mut_ptr(), 64);
    if String::from_utf16_lossy(&text[..n as usize]) == search.0 && IsWindowVisible(h) != 0 {
        search.1 = h as usize;
    }
    1
}
unsafe fn drop_rect(p: POINT) -> RECT {
    let monitor = MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST);
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..zeroed()
    };
    GetMonitorInfoW(monitor, &mut info);
    let a = info.rcWork;
    let w = 1100.min(a.right - a.left);
    let h = 800.min(a.bottom - a.top);
    let x = (p.x - 100).clamp(a.left, a.right - w);
    let y = (p.y - 20).clamp(a.top, a.bottom - h);
    RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}
unsafe extern "system" fn strip_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match crate::guard::window("strip", || unsafe { strip_body(h, m, w, l) }) {
        Some(value) => value,
        None => DefWindowProcW(h, m, w, l),
    }
}
unsafe fn strip_body(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let controller = GetWindowLongPtrW(h, GWLP_USERDATA) as usize;
    let handled = HOSTS.with(|hosts| {
        let Ok(mut map) = hosts.try_borrow_mut() else {
            return false;
        };
        let Some(host) = map.get_mut(&controller) else {
            return false;
        };
        let Some(frame) = host.strips.iter().find_map(|(f, s)| {
            if s.window == h as usize {
                Some(*f)
            } else {
                None
            }
        }) else {
            return false;
        };
        if m == WM_PAINT {
            host.strips[&frame].paint(host.excel);
            return true;
        }
        if m == WM_ERASEBKGND {
            return true;
        }
        if m == WM_TIMER {
            let Some(s) = host.strips.get_mut(&frame) else {
                return false;
            };
            let dt = s.last.elapsed().as_secs_f32();
            s.last = Instant::now();
            let mut moving = false;
            for (x, r) in s.positions.iter_mut().zip(&s.rects) {
                *x = ease(*x, r.x as f32, dt);
                moving |= *x != r.x as f32;
            }
            if !moving {
                KillTimer(h, 2);
            }
            InvalidateRect(h, null(), 0);
            return true;
        }
        if m == WM_SIZE {
            let Some(s) = host.strips.get_mut(&frame) else {
                return false;
            };
            s.layout();
            s.positions = s.rects.iter().map(|r| r.x as f32).collect();
            InvalidateRect(h, null(), 0);
            return true;
        }
        if m == WM_LBUTTONDOWN {
            let point = cursor();
            let mut local = point;
            ScreenToClient(h, &mut local);
            let right = client_rect(h).right;
            let Some(s) = host.strips.get_mut(&frame) else {
                return false;
            };
            if local.x > right - 105 {
                if local.x < right - 69 {
                    host.enqueue(Command::New);
                } else {
                    s.offset += if local.x < right - 35 { -160 } else { 160 };
                    s.layout();
                    s.positions = s.rects.iter().map(|r| r.x as f32).collect();
                    InvalidateRect(h, null(), 0);
                }
                return true;
            }
            for (i, r) in s.rects.iter().enumerate() {
                let mut actual = *r;
                actual.x = s.positions.get(i).copied().unwrap_or(r.x as f32) as i32;
                if actual.contains(local.x, local.y) {
                    let id = s.docs[i].id;
                    if local.x >= actual.x + actual.width - 27 {
                        host.enqueue(Command::Close(id));
                    } else {
                        s.drag.start(id, point.x, point.y);
                        SetFocus(h);
                        SetCapture(h);
                    }
                    break;
                }
            }
            return true;
        }
        if m == WM_MOUSEMOVE {
            let point = cursor();
            let mut local = point;
            ScreenToClient(h, &mut local);
            let Some(s) = host.strips.get_mut(&frame) else {
                return false;
            };
            if s.drag.document.is_some() {
                s.drag
                    .update(point.x, point.y, GetSystemMetrics(SM_CXDRAG) / 2);
                if s.drag.moved {
                    let r = client_rect(h);
                    if local.y >= -16
                        && local.y < r.bottom + 16
                        && local.x >= -16
                        && local.x < r.right + 16
                    {
                        s.marker = Some(s.before(local.x).unwrap_or(s.docs.len()));
                        host.hide_preview();
                    } else {
                        s.marker = None;
                        host.preview(frame, point);
                    }
                    InvalidateRect(h, null(), 0);
                }
                return true;
            }
        }
        if m == WM_LBUTTONUP {
            let point = cursor();
            let mut local = point;
            ScreenToClient(h, &mut local);
            let target = host.target(frame, point);
            let Some(s) = host.strips.get_mut(&frame) else {
                return false;
            };
            let r = client_rect(h);
            let inside = local.y >= -16
                && local.y < r.bottom + 16
                && local.x >= -16
                && local.x < r.right + 16;
            let before = s.before(local.x).and_then(|i| s.docs.get(i)).map(|d| d.id);
            let action = s.drag.finish(point.x, point.y, inside, before, target);
            s.marker = None;
            ReleaseCapture();
            host.hide_preview();
            host.enqueue(Command::Drop(action));
            return true;
        }
        if m == WM_CAPTURECHANGED || (m == WM_KEYDOWN && w == VK_ESCAPE as usize) {
            let Some(s) = host.strips.get_mut(&frame) else {
                return false;
            };
            s.drag.cancel();
            s.marker = None;
            host.hide_preview();
            if m == WM_KEYDOWN {
                ReleaseCapture();
            }
            InvalidateRect(h, null(), 0);
            return true;
        }
        false
    });
    if handled {
        0
    } else {
        DefWindowProcW(h, m, w, l)
    }
}
unsafe extern "system" fn frame_proc(
    h: HWND,
    m: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    controller: usize,
) -> LRESULT {
    match crate::guard::window("frame", || unsafe {
        frame_body(h, m, w, l, id, controller)
    }) {
        Some(value) => value,
        None => DefSubclassProc(h, m, w, l),
    }
}
unsafe fn frame_body(
    h: HWND,
    m: u32,
    w: WPARAM,
    l: LPARAM,
    _: usize,
    controller: usize,
) -> LRESULT {
    let block = HOSTS.with(|hosts| {
        let Ok(mut map) = hosts.try_borrow_mut() else {
            return false;
        };
        let Some(host) = map.get_mut(&controller) else {
            return false;
        };
        if m == WM_SYSCOMMAND
            && (w as u32 & 0xfff0) == SC_CLOSE
            && !host.bypass
            && !host.model.independent.contains(&(h as usize))
            && host.model.view(h as usize).len() > 1
        {
            if !host.closing {
                host.enqueue(Command::CloseAll(h as usize));
            }
            return true;
        }
        if m == WM_ENTERSIZEMOVE {
            if let Some(s) = host.strips.get_mut(&(h as usize)) {
                s.start = rect(h);
                s.moving = true;
            }
        }
        if m == WM_MOVING && host.model.independent.contains(&(h as usize)) {
            host.preview(h as usize, cursor());
        }
        if m == WM_EXITSIZEMOVE {
            if let Some(s) = host.strips.get_mut(&(h as usize)) {
                s.moving = false;
                let changed = !same(s.start, rect(h));
                if changed && host.model.independent.contains(&(h as usize)) {
                    let p = cursor();
                    if let Some(target) = host.target(h as usize, p) {
                        if let Some(id) = host
                            .model
                            .documents
                            .iter()
                            .find(|d| d.frame == h as usize)
                            .map(|d| d.id)
                        {
                            host.enqueue(Command::Drop(DropAction::Dock(id, target)));
                        }
                    }
                }
            }
            host.hide_preview();
            PostMessageW(hwnd(controller), DIRTY, 0, 0);
        }
        if m == WM_ACTIVATE || m == WM_SIZE || m == WM_DPICHANGED {
            PostMessageW(hwnd(controller), DIRTY, 0, 0);
        }
        false
    });
    if block {
        0
    } else {
        DefSubclassProc(h, m, w, l)
    }
}
unsafe extern "system" fn workspace_proc(
    h: HWND,
    m: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    controller: usize,
) -> LRESULT {
    match crate::guard::window("workspace", || unsafe {
        workspace_body(h, m, w, l, id, controller)
    }) {
        Some(value) => value,
        None => DefSubclassProc(h, m, w, l),
    }
}
unsafe fn workspace_body(
    h: HWND,
    m: u32,
    w: WPARAM,
    l: LPARAM,
    _: usize,
    controller: usize,
) -> LRESULT {
    if m == WM_WINDOWPOSCHANGING && l != 0 {
        HOSTS.with(|hosts| {
            let Ok(mut map) = hosts.try_borrow_mut() else {
                return;
            };
            let Some(host) = map.get_mut(&controller) else {
                return;
            };
            let Some(s) = host.strips.values_mut().find(|s| s.workspace == h as usize) else {
                return;
            };
            let p = &mut *(l as *mut WINDOWPOS);
            let height = s.applied.top - s.original.top;
            if p.flags & SWP_NOMOVE == 0 && p.flags & SWP_NOSIZE == 0 && p.cy > height + 100 {
                if p.y != s.applied.top {
                    s.original = RECT {
                        left: p.x,
                        top: p.y,
                        right: p.x + p.cx,
                        bottom: p.y + p.cy,
                    };
                    p.y += height;
                    p.cy -= height;
                }
                s.applied = RECT {
                    left: p.x,
                    top: p.y,
                    right: p.x + p.cx,
                    bottom: p.y + p.cy,
                };
                SetWindowPos(
                    hwnd(s.window),
                    null_mut(),
                    p.x,
                    p.y - height,
                    p.cx,
                    height,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
        });
    }
    DefSubclassProc(h, m, w, l)
}

#[cfg(test)]
mod dock_tests {
    //! Guards the rule that decides which Office window gets tabs. It broke
    //! twice: once on Excel's start screen, once for frames hidden by grouping.
    use super::{counts_as_document, workspace_visible};
    use std::{mem::zeroed, ptr::null_mut, sync::Once};
    use windows_sys::Win32::{
        Foundation::*, System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::*,
    };
    static CLASSES: Once = Once::new();
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    unsafe fn class(name: &str) {
        let text = wide(name);
        let mut c: WNDCLASSEXW = zeroed();
        c.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        c.lpfnWndProc = Some(DefWindowProcW);
        c.hInstance = GetModuleHandleW(std::ptr::null());
        c.lpszClassName = text.as_ptr();
        RegisterClassExW(&c);
    }
    unsafe fn create(class_name: &'static str, parent: HWND, style: u32) -> HWND {
        CLASSES.call_once(|| {
            class("OfficeTabs.DockTest.Frame");
            class("XLDESK");
            class("_WwB");
        });
        let name = wide(class_name);
        let title = wide("dock test");
        CreateWindowExW(
            0,
            name.as_ptr(),
            title.as_ptr(),
            style,
            0,
            0,
            400,
            300,
            parent,
            null_mut(),
            GetModuleHandleW(std::ptr::null()),
            null_mut(),
        )
    }
    unsafe fn frame() -> HWND {
        let h = create("OfficeTabs.DockTest.Frame", null_mut(), WS_OVERLAPPEDWINDOW);
        assert!(!h.is_null(), "frame window was not created");
        ShowWindow(h, SW_SHOW);
        h
    }
    unsafe fn child(parent: HWND, class: &'static str) -> HWND {
        let h = create(class, parent, WS_CHILD | WS_VISIBLE);
        assert!(!h.is_null(), "child window was not created");
        h
    }
    #[test]
    fn excel_start_screen_has_no_document_area() {
        unsafe {
            let f = frame();
            assert!(!workspace_visible(f as usize, true));
            assert!(!counts_as_document(f as usize, true));
            DestroyWindow(f);
        }
    }
    #[test]
    fn a_visible_grid_counts_and_its_removal_stops_counting() {
        unsafe {
            let f = frame();
            assert!(!counts_as_document(f as usize, true));
            let grid = child(f, "XLDESK");
            assert!(workspace_visible(f as usize, true));
            assert!(counts_as_document(f as usize, true));
            ShowWindow(grid, SW_HIDE);
            assert!(!counts_as_document(f as usize, true));
            DestroyWindow(f);
        }
    }
    #[test]
    fn word_ignores_excel_classes_and_vice_versa() {
        unsafe {
            let f = frame();
            child(f, "XLDESK");
            assert!(!counts_as_document(f as usize, false));
            let w = frame();
            child(w, "_WwB");
            assert!(counts_as_document(w as usize, false));
            assert!(!counts_as_document(w as usize, true));
            DestroyWindow(f);
            DestroyWindow(w);
        }
    }
    #[test]
    fn a_hidden_frame_is_a_tab_of_the_group() {
        unsafe {
            let f = frame();
            ShowWindow(f, SW_HIDE);
            assert!(!workspace_visible(f as usize, true));
            assert!(counts_as_document(f as usize, true));
            DestroyWindow(f);
        }
    }
}
