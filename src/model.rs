// Pure, deterministic state machine. No HWND, COM or UI calls here.
use std::collections::{BTreeSet, HashMap, HashSet};
pub type DocumentId = usize;
pub type FrameId = usize;
#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub id: DocumentId,
    pub name: String,
    pub saved: bool,
    pub active: bool,
    pub frame: FrameId,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}
impl Bounds {
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x
            && y >= self.y
            && (x as i64) < self.x as i64 + self.width as i64
            && (y as i64) < self.y as i64 + self.height as i64
    }
}
/// Recover the reserved tab band after a host relayout, without subtracting twice.
pub fn reserve_workspace(current: Bounds, applied: Bounds, band: i32) -> Option<(Bounds, Bounds)> {
    if current.width <= 0 || current.height < 100 || band <= 0 {
        return None;
    }
    let mut original = current;
    if current.y == applied.y {
        original.y -= band;
        original.height += band;
    }
    if original.height <= band + 100 {
        return None;
    }
    let mut reserved = original;
    reserved.y += band;
    reserved.height -= band;
    Some((original, reserved))
}
#[derive(Default)]
pub struct Model {
    pub documents: Vec<Document>,
    order: Vec<DocumentId>,
    pub independent: BTreeSet<FrameId>,
    pub group_frame: Option<FrameId>,
}
impl Model {
    /// Rank table for the remembered tab order; unknown ids sort last.
    fn ranks(&self) -> HashMap<DocumentId, usize> {
        self.order
            .iter()
            .enumerate()
            .map(|(at, id)| (*id, at))
            .collect()
    }
    pub fn snapshot(&mut self, documents: Vec<Document>) {
        let mut known: HashSet<DocumentId> = self.order.iter().copied().collect();
        for d in &documents {
            if known.insert(d.id) {
                self.order.push(d.id);
            }
        }
        self.documents = documents;
        let ranks = self.ranks();
        self.documents
            .sort_by_key(|d| ranks.get(&d.id).copied().unwrap_or(usize::MAX));
    }
    /// Borrowing view; callers that only compare avoid a per-frame clone.
    pub fn view_iter(&self, frame: FrameId) -> impl Iterator<Item = &Document> {
        let own = self.independent.contains(&frame);
        self.documents.iter().filter(move |d| {
            if own {
                d.frame == frame
            } else {
                !self.independent.contains(&d.frame)
            }
        })
    }
    pub fn view(&self, frame: FrameId) -> Vec<Document> {
        self.view_iter(frame).cloned().collect()
    }
    pub fn reorder(&mut self, id: DocumentId, before: Option<DocumentId>) {
        if before == Some(id)
            || !self.order.contains(&id)
            || before.is_some_and(|v| !self.order.contains(&v))
        {
            return;
        }
        self.order.retain(|v| *v != id);
        let at = before
            .and_then(|v| self.order.iter().position(|p| *p == v))
            .unwrap_or(self.order.len());
        self.order.insert(at, id);
        let ranks = self.ranks();
        self.documents
            .sort_by_key(|d| ranks.get(&d.id).copied().unwrap_or(usize::MAX));
    }
    pub fn detach(&mut self, frame: FrameId) {
        self.independent.insert(frame);
        if self.group_frame == Some(frame) {
            self.group_frame = None;
        }
    }
    pub fn dock(&mut self, source: FrameId, target: FrameId) {
        self.independent.remove(&source);
        self.independent.remove(&target);
        self.group_frame = Some(target);
    }
    pub fn prune_frames(&mut self, exists: impl Fn(FrameId) -> bool) {
        self.independent.retain(|f| exists(*f));
        if self.group_frame.is_some_and(|f| !exists(f)) {
            self.group_frame = None;
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DropAction {
    Cancel,
    Activate(DocumentId),
    Reorder(DocumentId, Option<DocumentId>),
    Detach(DocumentId, i32, i32),
    Dock(DocumentId, FrameId),
}
#[derive(Default)]
pub struct Drag {
    pub document: Option<DocumentId>,
    pub start: (i32, i32),
    pub moved: bool,
}
impl Drag {
    pub fn start(&mut self, id: DocumentId, x: i32, y: i32) {
        self.document = Some(id);
        self.start = (x, y);
        self.moved = false;
    }
    pub fn update(&mut self, x: i32, y: i32, threshold: i32) {
        self.moved |= (x as i64 - self.start.0 as i64).abs() > threshold as i64
            || (y as i64 - self.start.1 as i64).abs() > threshold as i64;
    }
    pub fn cancel(&mut self) {
        *self = Self::default();
    }
    pub fn finish(
        &mut self,
        x: i32,
        y: i32,
        inside: bool,
        before: Option<DocumentId>,
        target: Option<FrameId>,
    ) -> DropAction {
        let result = match self.document {
            None => DropAction::Cancel,
            Some(id) if !self.moved => DropAction::Activate(id),
            Some(id) if inside => DropAction::Reorder(id, before),
            Some(id) => match target {
                Some(frame) => DropAction::Dock(id, frame),
                None => DropAction::Detach(id, x, y),
            },
        };
        self.cancel();
        result
    }
}
pub fn tab_widths(preferred: &[i32], available: i32) -> Vec<i32> {
    if preferred.iter().sum::<i32>() <= available {
        return preferred.to_vec();
    }
    let mut lo = 72;
    let mut hi = preferred.iter().copied().max().unwrap_or(72);
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        if preferred.iter().map(|v| (*v).min(mid)).sum::<i32>() <= available {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    preferred.iter().map(|v| (*v).min(lo)).collect()
}
pub fn ease(current: f32, target: f32, seconds: f32) -> f32 {
    let value = current + (target - current) * (1.0 - (-24.0 * seconds.max(0.0)).exp());
    if (value - target).abs() < 0.5 {
        target
    } else {
        value
    }
}
#[derive(Default, Debug)]
pub struct Progress {
    pub total: i64,
    pub ticks: i64,
    pub reverse: bool,
    pub script: bool,
    pub action_ticks: i64,
    pub action_data: bool,
}
impl Progress {
    pub fn message(&mut self, kind: i32, a: i32, b: i32, c: i32) {
        match kind {
            0 => {
                self.total = (a as i64).max(0);
                self.reverse = b != 0;
                self.script = c != 0;
                self.ticks = if self.reverse { self.total } else { 0 };
                self.action_data = false;
            }
            1 => {
                self.action_ticks = (a as i64).max(0);
                self.action_data = b != 0;
            }
            2 => self.advance(a as i64),
            3 => self.total = (self.total + a as i64).max(0),
            _ => {}
        }
    }
    pub fn advance(&mut self, value: i64) {
        self.ticks =
            (self.ticks + if self.reverse { -value } else { value }).clamp(0, self.total.max(0));
    }
    pub fn action(&mut self) {
        if self.action_data {
            self.advance(self.action_ticks);
        }
    }
    pub fn percent(&self) -> Option<u32> {
        if self.total <= 0 {
            None
        } else {
            Some((self.ticks * 100 / self.total).clamp(0, 100) as u32)
        }
    }
}
