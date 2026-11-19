//! The tab strip window: one child window per Office frame, positioned over the
//! document area. It draws itself from the document list it is handed and knows
//! nothing about COM, so it can be reasoned about on its own.
use crate::{
    model::{reserve_workspace, tab_widths, Bounds, Document, Drag},
    native::{client_rect, hwnd, parent_rect, same, wide},
};
use std::{
    collections::HashMap,
    ptr::{null, null_mut},
    time::Instant,
};
use windows_sys::Win32::{Foundation::*, Graphics::Gdi::*, UI::WindowsAndMessaging::*};

pub(crate) struct Strip {
    pub(crate) window: usize,
    pub(crate) workspace: usize,
    pub(crate) parent: usize,
    pub(crate) original: RECT,
    pub(crate) applied: RECT,
    pub(crate) docs: Vec<Document>,
    pub(crate) rects: Vec<Bounds>,
    pub(crate) positions: Vec<f32>,
    /// Pre-built tab captions so repaints during animation allocate nothing.
    pub(crate) labels: Vec<Vec<u16>>,
    pub(crate) last: Instant,
    pub(crate) drag: Drag,
    pub(crate) offset: i32,
    pub(crate) marker: Option<usize>,
    pub(crate) font: HFONT,
    pub(crate) measures: HashMap<String, i32>,
    pub(crate) moving: bool,
    pub(crate) start: RECT,
}
impl Strip {
    pub(crate) unsafe fn reconcile_workspace(&mut self) {
        let current = parent_rect(hwnd(self.workspace), hwnd(self.parent));
        let to_bounds = |r: RECT| Bounds {
            x: r.left,
            y: r.top,
            width: r.right - r.left,
            height: r.bottom - r.top,
        };
        let to_rect = |r: Bounds| RECT {
            left: r.x,
            top: r.y,
            right: r.x + r.width,
            bottom: r.y + r.height,
        };
        let height = self.applied.top - self.original.top;
        let Some((original, applied)) =
            reserve_workspace(to_bounds(current), to_bounds(self.applied), height)
        else {
            return;
        };
        self.original = to_rect(original);
        self.applied = to_rect(applied);
        let changed = !same(current, self.applied);
        if changed {
            SetWindowPos(
                hwnd(self.workspace),
                null_mut(),
                applied.x,
                applied.y,
                applied.width,
                applied.height,
                SWP_NOACTIVATE | SWP_NOZORDER,
            );
        }
        let band = RECT {
            left: original.x,
            top: original.y,
            right: original.x + original.width,
            bottom: original.y + height,
        };
        if changed
            || !same(parent_rect(hwnd(self.window), hwnd(self.parent)), band)
            || IsWindowVisible(hwnd(self.window)) == 0
        {
            SetWindowPos(
                hwnd(self.window),
                HWND_TOP,
                band.left,
                band.top,
                original.width,
                height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            self.layout();
            self.positions = self.rects.iter().map(|r| r.x as f32).collect();
            InvalidateRect(hwnd(self.window), null(), 0);
        }
    }
    pub(crate) unsafe fn update(&mut self, docs: Vec<Document>) {
        let old: HashMap<usize, f32> = self
            .docs
            .iter()
            .zip(&self.positions)
            .map(|(d, x)| (d.id, *x))
            .collect();
        self.docs = docs;
        self.layout();
        self.positions = self
            .docs
            .iter()
            .zip(&self.rects)
            .map(|(d, r)| old.get(&d.id).copied().unwrap_or(r.x as f32))
            .collect();
        self.last = Instant::now();
        if IsWindowVisible(hwnd(self.window)) != 0 {
            SetTimer(hwnd(self.window), 2, 16, None);
        } else {
            self.positions = self.rects.iter().map(|r| r.x as f32).collect();
        }
        InvalidateRect(hwnd(self.window), null(), 0);
    }
    pub(crate) unsafe fn layout(&mut self) {
        let h = hwnd(self.window);
        let client = client_rect(h);
        let dc = GetDC(h);
        let old = SelectObject(dc, self.font);
        let mut widths = Vec::with_capacity(self.docs.len());
        self.labels.clear();
        for doc in &self.docs {
            let width = *self.measures.entry(doc.name.clone()).or_insert_with(|| {
                let text = wide(&doc.name);
                let mut sz = SIZE::default();
                GetTextExtentPoint32W(dc, text.as_ptr(), text.len() as i32 - 1, &mut sz);
                sz.cx
            });
            widths.push((width + 54).max(88));
            self.labels.push(if doc.saved {
                wide(&doc.name)
            } else {
                wide(&format!("● {}", doc.name))
            });
        }
        SelectObject(dc, old);
        ReleaseDC(h, dc);
        let widths = tab_widths(&widths, (client.right - 110).max(1));
        self.offset = self.offset.clamp(
            0,
            (widths.iter().sum::<i32>() - (client.right - 110)).max(0),
        );
        let mut x = 6 - self.offset;
        self.rects = widths
            .iter()
            .map(|w| {
                let r = Bounds {
                    x,
                    y: 5,
                    width: w - 5,
                    height: (client.bottom - 9).max(28),
                };
                x += w;
                r
            })
            .collect();
        // Renamed documents keep their measured width; drop the table when it
        // outgrows the documents on screen so a long session cannot grow it.
        if self.measures.len() > 64 + 4 * self.docs.len() {
            self.measures.clear();
        }
    }
    pub(crate) unsafe fn paint(&self, excel: bool) {
        let h = hwnd(self.window);
        let mut ps = PAINTSTRUCT::default();
        let dc = BeginPaint(h, &mut ps);
        let r = client_rect(h);
        if r.right <= 0 || r.bottom <= 0 {
            EndPaint(h, &ps);
            return;
        }
        let buffer = CreateCompatibleDC(dc);
        let bitmap = CreateCompatibleBitmap(dc, r.right, r.bottom);
        let old_bitmap = SelectObject(buffer, bitmap);
        // One brush per role per repaint instead of one per tab.
        let background = CreateSolidBrush(0x00f8f2ed);
        let idle = CreateSolidBrush(0x00f3ede7);
        let active = CreateSolidBrush(0x00ffffff);
        let accent = CreateSolidBrush(if excel { 0x004a7516 } else { 0x00a45522 });
        let marker = CreateSolidBrush(0x00a45522);
        FillRect(buffer, &r, background);
        let font = SelectObject(buffer, self.font);
        SetBkMode(buffer, TRANSPARENT as i32);
        SetTextColor(buffer, 0x00503019);
        let saved = SaveDC(buffer);
        IntersectClipRect(buffer, 3, 0, (r.right - 105).max(4), r.bottom);
        let cross = [0x00d7u16, 0];
        for (i, doc) in self.docs.iter().enumerate() {
            let Some(target) = self.rects.get(i) else {
                continue;
            };
            let Some(label) = self.labels.get(i) else {
                continue;
            };
            let x = self
                .positions
                .get(i)
                .copied()
                .unwrap_or(target.x as f32)
                .round() as i32;
            let mut rr = RECT {
                left: x,
                top: target.y,
                right: x + target.width,
                bottom: target.y + target.height,
            };
            if rr.right < 3 || rr.left > r.right - 105 {
                continue;
            }
            FillRect(buffer, &rr, if doc.active { active } else { idle });
            if doc.active {
                FillRect(
                    buffer,
                    &RECT {
                        left: rr.left + 8,
                        right: rr.right - 8,
                        top: rr.bottom - 3,
                        bottom: rr.bottom,
                    },
                    accent,
                );
            }
            rr.left += 10;
            rr.right -= 28;
            DrawTextW(
                buffer,
                label.as_ptr(),
                -1,
                &mut rr,
                DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
            );
            let mut close = RECT {
                left: x + target.width - 27,
                right: x + target.width,
                top: target.y,
                bottom: target.y + target.height,
            };
            DrawTextW(
                buffer,
                cross.as_ptr(),
                -1,
                &mut close,
                DT_SINGLELINE | DT_VCENTER | DT_CENTER,
            );
        }
        if let Some(at) = self.marker {
            let x = self
                .rects
                .get(at)
                .map(|r| r.x)
                .unwrap_or_else(|| self.rects.last().map(|r| r.x + r.width + 5).unwrap_or(6));
            FillRect(
                buffer,
                &RECT {
                    left: x,
                    top: 4,
                    right: x + 3,
                    bottom: r.bottom - 4,
                },
                marker,
            );
        }
        RestoreDC(buffer, saved);
        let plus = [0x2bu16, 0];
        let previous = [0x2039u16, 0];
        let next = [0x203au16, 0];
        for (label, x) in [
            (plus.as_ptr(), r.right - 99),
            (previous.as_ptr(), r.right - 65),
            (next.as_ptr(), r.right - 33),
        ] {
            let mut rr = RECT {
                left: x,
                top: 0,
                right: x + 30,
                bottom: r.bottom,
            };
            DrawTextW(
                buffer,
                label,
                -1,
                &mut rr,
                DT_SINGLELINE | DT_CENTER | DT_VCENTER,
            );
        }
        SelectObject(buffer, font);
        BitBlt(dc, 0, 0, r.right, r.bottom, buffer, 0, 0, SRCCOPY);
        SelectObject(buffer, old_bitmap);
        DeleteObject(bitmap);
        DeleteDC(buffer);
        DeleteObject(background);
        DeleteObject(idle);
        DeleteObject(active);
        DeleteObject(accent);
        DeleteObject(marker);
        EndPaint(h, &ps);
    }
    pub(crate) fn before(&self, x: i32) -> Option<usize> {
        self.rects.iter().position(|r| x < r.x + r.width / 2)
    }
}
