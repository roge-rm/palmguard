//! Pure palm/stray-touch rejection logic, independent of any device I/O.
//!
//! Input: the raw multitouch protocol-B stream from the touchscreen
//! (`abs()` for each EV_ABS, `syn()` for each SYN_REPORT).
//! Output: a filtered protocol-B stream for the virtual touchscreen, where
//! pen contacts are labelled MT_TOOL_PEN and unwanted contacts are dropped
//! or cancelled (by relabelling them MT_TOOL_PALM before lifting, which
//! Android turns into ACTION_CANCEL / FLAG_CANCELED).

pub const MAX_SLOTS: usize = 16;

pub const ABS_MT_SLOT: u16 = 0x2f;
pub const ABS_MT_TOUCH_MAJOR: u16 = 0x30;
pub const ABS_MT_POSITION_X: u16 = 0x35;
pub const ABS_MT_POSITION_Y: u16 = 0x36;
pub const ABS_MT_TOOL_TYPE: u16 = 0x37;
pub const ABS_MT_TRACKING_ID: u16 = 0x39;
pub const ABS_MT_PRESSURE: u16 = 0x3a;

pub const BTN_TOOL_PEN: u16 = 0x140;
pub const BTN_TOOL_FINGER: u16 = 0x145;
pub const BTN_TOUCH: u16 = 0x14a;

pub const MT_TOOL_FINGER: i32 = 0;
pub const MT_TOOL_PEN: i32 = 1;
pub const MT_TOOL_PALM: i32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Contact {
    pub id: i32,
    pub x: i32,
    pub y: i32,
    pub major: i32,
    pub pressure: i32,
    pub tool: i32,
}

impl Contact {
    pub const EMPTY: Contact = Contact { id: -1, x: 0, y: 0, major: 0, pressure: 0, tool: 0 };
    pub fn active(&self) -> bool {
        self.id >= 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Pen,
    Touch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fate {
    /// Held back for `holdoff_ms` in case a pen lands right after it.
    Pending,
    /// Forwarded to Android.
    Pass,
    /// Never forwarded (or cancelled), until it lifts.
    Reject,
}

#[derive(Clone, Copy, Debug)]
struct Track {
    id: i32,
    class: Class,
    fate: Fate,
    start_ms: u64,
}

#[derive(Clone, Debug)]
pub struct Config {
    /// Contacts whose first TOUCH_MAJOR is at or below this are the pen.
    pub pen_max_major: i32,
    /// A pen contact that grows past this is re-graded as a hand and cancelled.
    pub pen_regrade_major: i32,
    /// Non-pen contacts starting within this long after the pen lifts are rejected.
    pub grace_ms: u64,
    /// Non-pen contacts are held this long before being forwarded, so a hand
    /// that lands just before the pen never reaches the app.
    pub holdoff_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config { pen_max_major: 80, pen_regrade_major: 150, grace_ms: 500, holdoff_ms: 40 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Out {
    Abs(u16, i32),
    Key(u16, i32),
    Syn,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Note {
    Start { slot: usize, id: i32, major: i32, class: Class, fate: Fate, why: &'static str },
    Promote { id: i32 },
    Drop { id: i32, why: &'static str },
    Cancel { id: i32, why: &'static str },
    Regrade { id: i32, major: i32 },
    End { id: i32, class: Class, fate: Fate },
}

pub struct Filter {
    pub cfg: Config,
    nslots: usize,
    active: bool,
    hw: [Contact; MAX_SLOTS],
    cur: usize,
    track: [Option<Track>; MAX_SLOTS],
    out: [Contact; MAX_SLOTS],
    out_slot: i32,
    key_touch: i32,
    key_pen: i32,
    key_finger: i32,
    last_pen_up: Option<u64>,
    pub notes: Vec<Note>,
}

impl Filter {
    pub fn new(cfg: Config, nslots: usize) -> Self {
        Filter {
            cfg,
            nslots: nslots.clamp(1, MAX_SLOTS),
            active: true,
            hw: [Contact::EMPTY; MAX_SLOTS],
            cur: 0,
            track: [None; MAX_SLOTS],
            out: [Contact::EMPTY; MAX_SLOTS],
            out_slot: -1,
            key_touch: 0,
            key_pen: 0,
            key_finger: 0,
            last_pen_up: None,
            notes: Vec::new(),
        }
    }

    /// When inactive, new contacts are passed through unchanged (as fingers).
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    pub fn hw_contacts(&self) -> usize {
        self.hw[..self.nslots].iter().filter(|c| c.active()).count()
    }

    /// Seed per-slot hardware state (from EVIOCGMTSLOTS), e.g. at startup
    /// or after SYN_DROPPED.
    pub fn seed(&mut self, code: u16, values: &[i32]) {
        for (s, &v) in values.iter().enumerate().take(self.nslots) {
            self.set_field(s, code, v);
        }
    }

    pub fn seed_slot(&mut self, slot: i32) {
        if slot >= 0 {
            self.cur = (slot as usize).min(self.nslots - 1);
        }
    }

    /// Call when the virtual device is known to be idle (all contacts up),
    /// e.g. right before (re)grabbing the real touchscreen.
    pub fn forget_output(&mut self) {
        self.out = [Contact::EMPTY; MAX_SLOTS];
        self.out_slot = -1;
        self.key_touch = 0;
        self.key_pen = 0;
        self.key_finger = 0;
    }

    fn set_field(&mut self, s: usize, code: u16, v: i32) {
        let c = &mut self.hw[s];
        match code {
            ABS_MT_TRACKING_ID => c.id = if v < 0 { -1 } else { v },
            ABS_MT_POSITION_X => c.x = v,
            ABS_MT_POSITION_Y => c.y = v,
            ABS_MT_TOUCH_MAJOR => c.major = v,
            ABS_MT_PRESSURE => c.pressure = v,
            ABS_MT_TOOL_TYPE => c.tool = v,
            _ => {}
        }
    }

    pub fn abs(&mut self, code: u16, value: i32) {
        if code == ABS_MT_SLOT {
            self.seed_slot(value);
        } else {
            self.set_field(self.cur, code, value);
        }
    }

    fn pen_down(&self) -> bool {
        (0..self.nslots).any(|s| {
            matches!(self.track[s], Some(t) if t.class == Class::Pen && t.fate == Fate::Pass)
                && self.hw[s].active()
        })
    }

    fn in_grace(&self, now: u64) -> bool {
        matches!(self.last_pen_up, Some(t) if now.saturating_sub(t) < self.cfg.grace_ms)
    }

    fn end(&mut self, s: usize, now: u64) {
        if let Some(t) = self.track[s].take() {
            if t.class == Class::Pen && t.fate == Fate::Pass {
                self.last_pen_up = Some(now);
            }
            self.notes.push(Note::End { id: t.id, class: t.class, fate: t.fate });
        }
    }

    /// Process one hardware frame (SYN_REPORT).
    pub fn syn(&mut self, now: u64, out: &mut Vec<Out>) {
        let n = self.nslots;
        let mut cancels: Vec<usize> = Vec::new();

        // 1. Contacts that lifted (or were replaced in the same slot).
        for s in 0..n {
            if let Some(t) = self.track[s] {
                if self.hw[s].id != t.id {
                    self.end(s, now);
                }
            }
        }

        // 2. New contacts. Pens first, so a hand landing in the same frame
        //    as the pen is already rejected.
        let mut new_pen = false;
        for pens_pass in [true, false] {
            for s in 0..n {
                let c = self.hw[s];
                if !c.active() || self.track[s].is_some() {
                    continue;
                }
                let is_pen = self.active && c.major <= self.cfg.pen_max_major && c.tool != MT_TOOL_PALM;
                if is_pen != pens_pass {
                    continue;
                }
                let (class, fate, why) = if is_pen {
                    new_pen = true;
                    (Class::Pen, Fate::Pass, "pen")
                } else if !self.active {
                    (Class::Touch, Fate::Pass, "inactive")
                } else if self.pen_down() {
                    (Class::Touch, Fate::Reject, "pen down")
                } else if self.in_grace(now) {
                    (Class::Touch, Fate::Reject, "grace")
                } else if self.cfg.holdoff_ms > 0 {
                    (Class::Touch, Fate::Pending, "holdoff")
                } else {
                    (Class::Touch, Fate::Pass, "touch")
                };
                self.track[s] = Some(Track { id: c.id, class, fate, start_ms: now });
                self.notes.push(Note::Start { slot: s, id: c.id, major: c.major, class, fate, why });
            }
        }

        // 3. A "pen" that grows (or the firmware calls a palm) is a hand.
        for s in 0..n {
            if let Some(t) = self.track[s].as_mut() {
                let c = self.hw[s];
                if t.class == Class::Pen
                    && t.fate == Fate::Pass
                    && (c.major > self.cfg.pen_regrade_major || c.tool == MT_TOOL_PALM)
                {
                    t.class = Class::Touch;
                    t.fate = Fate::Reject;
                    self.notes.push(Note::Regrade { id: t.id, major: c.major });
                    cancels.push(s);
                }
            }
        }

        // 4. A pen landing kills every other contact.
        if new_pen {
            for s in 0..n {
                if let Some(t) = self.track[s].as_mut() {
                    if t.class != Class::Touch {
                        continue;
                    }
                    match t.fate {
                        Fate::Pass => {
                            t.fate = Fate::Reject;
                            self.notes.push(Note::Cancel { id: t.id, why: "pen landed" });
                            cancels.push(s);
                        }
                        Fate::Pending => {
                            t.fate = Fate::Reject;
                            self.notes.push(Note::Drop { id: t.id, why: "pen landed" });
                        }
                        Fate::Reject => {}
                    }
                }
            }
        }

        // 5. Held-back contacts whose hold-off has expired.
        self.promote(now);

        self.emit(&cancels, out);
    }

    /// Call periodically (poll timeout) so held-back contacts are released
    /// even if the hardware sends nothing. Returns the next deadline, if any.
    pub fn tick(&mut self, now: u64, out: &mut Vec<Out>) -> Option<u64> {
        if self.promote(now) {
            self.emit(&[], out);
        }
        self.deadline()
    }

    pub fn deadline(&self) -> Option<u64> {
        self.track[..self.nslots]
            .iter()
            .flatten()
            .filter(|t| t.fate == Fate::Pending)
            .map(|t| t.start_ms + self.cfg.holdoff_ms)
            .min()
    }

    fn promote(&mut self, now: u64) -> bool {
        let mut changed = false;
        let pen = self.pen_down();
        let grace = self.in_grace(now);
        for s in 0..self.nslots {
            if let Some(t) = self.track[s].as_mut() {
                if t.fate == Fate::Pending && now.saturating_sub(t.start_ms) >= self.cfg.holdoff_ms {
                    if pen || grace {
                        t.fate = Fate::Reject;
                        self.notes.push(Note::Drop { id: t.id, why: if pen { "pen down" } else { "grace" } });
                    } else {
                        t.fate = Fate::Pass;
                        self.notes.push(Note::Promote { id: t.id });
                        changed = true;
                    }
                }
            }
        }
        changed
    }

    fn desired(&self, s: usize) -> Contact {
        match self.track[s] {
            Some(t) if t.fate == Fate::Pass && self.hw[s].active() => {
                let mut c = self.hw[s];
                c.tool = match t.class {
                    Class::Pen => MT_TOOL_PEN,
                    Class::Touch if c.tool == MT_TOOL_PALM => MT_TOOL_PALM,
                    Class::Touch => MT_TOOL_FINGER,
                };
                c
            }
            _ => Contact::EMPTY,
        }
    }

    fn emit(&mut self, cancels: &[usize], out: &mut Vec<Out>) {
        let n = self.nslots;
        // Cancel frame: relabel as palm first so Android cancels instead of
        // delivering a normal lift (which could register as a tap).
        let mut want = self.out;
        let mut any_cancel = false;
        for &s in cancels {
            if self.out[s].active() && self.out[s].tool != MT_TOOL_PALM {
                want[s].tool = MT_TOOL_PALM;
                any_cancel = true;
            }
        }
        if any_cancel {
            self.frame(&want, out);
        }
        let mut want = [Contact::EMPTY; MAX_SLOTS];
        for (s, w) in want.iter_mut().enumerate().take(n) {
            *w = self.desired(s);
        }
        self.frame(&want, out);
    }

    fn frame(&mut self, want: &[Contact; MAX_SLOTS], out: &mut Vec<Out>) {
        let start = out.len();
        for s in 0..self.nslots {
            let d = want[s];
            let o = self.out[s];
            if d == o || (!d.active() && !o.active()) {
                continue;
            }
            if self.out_slot != s as i32 {
                out.push(Out::Abs(ABS_MT_SLOT, s as i32));
                self.out_slot = s as i32;
            }
            if !d.active() {
                out.push(Out::Abs(ABS_MT_TRACKING_ID, -1));
            } else if d.id != o.id {
                out.push(Out::Abs(ABS_MT_TRACKING_ID, d.id));
                out.push(Out::Abs(ABS_MT_TOOL_TYPE, d.tool));
                out.push(Out::Abs(ABS_MT_POSITION_X, d.x));
                out.push(Out::Abs(ABS_MT_POSITION_Y, d.y));
                out.push(Out::Abs(ABS_MT_TOUCH_MAJOR, d.major));
                out.push(Out::Abs(ABS_MT_PRESSURE, d.pressure));
            } else {
                if d.tool != o.tool {
                    out.push(Out::Abs(ABS_MT_TOOL_TYPE, d.tool));
                }
                if d.x != o.x {
                    out.push(Out::Abs(ABS_MT_POSITION_X, d.x));
                }
                if d.y != o.y {
                    out.push(Out::Abs(ABS_MT_POSITION_Y, d.y));
                }
                if d.major != o.major {
                    out.push(Out::Abs(ABS_MT_TOUCH_MAJOR, d.major));
                }
                if d.pressure != o.pressure {
                    out.push(Out::Abs(ABS_MT_PRESSURE, d.pressure));
                }
            }
            self.out[s] = d;
        }

        let live = || self.out[..self.nslots].iter().filter(|c| c.active() && c.tool != MT_TOOL_PALM);
        let touch = live().next().is_some() as i32;
        let pen = live().any(|c| c.tool == MT_TOOL_PEN) as i32;
        let finger = live().any(|c| c.tool != MT_TOOL_PEN) as i32;
        for (code, new, old) in [
            (BTN_TOOL_PEN, pen, &mut self.key_pen),
            (BTN_TOOL_FINGER, finger, &mut self.key_finger),
            (BTN_TOUCH, touch, &mut self.key_touch),
        ] {
            if new != *old {
                out.push(Out::Key(code, new));
                *old = new;
            }
        }

        if out.len() > start {
            out.push(Out::Syn);
        }
    }
}
