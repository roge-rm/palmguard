//! palmguard: stylus palm / stray-touch rejection daemon for passive-pen
//! touchscreens (built for the Moto G Stylus 2024, goodix_ts).
//!
//! Reads the real touchscreen, re-emits a filtered copy through a uinput
//! touchscreen with the pen labelled as a real stylus, and grabs the real
//! device so Android only sees the filtered one. The grab is only held while
//! the pen is out of its silo (by default), so docking the pen always gives
//! you the stock touchscreen back.

use palmguard::filter::*;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_ABS: u16 = 0x03;
const EV_SW: u16 = 0x05;
const SYN_REPORT: u16 = 0;
const SYN_DROPPED: u16 = 3;
const SW_PEN_INSERTED: u16 = 0x0f;
const KEY_CNT: usize = 0x300;
const INPUT_PROP_DIRECT: i32 = 0x01;

const MT_CODES: [u16; 6] = [
    ABS_MT_TRACKING_ID,
    ABS_MT_POSITION_X,
    ABS_MT_POSITION_Y,
    ABS_MT_TOUCH_MAJOR,
    ABS_MT_PRESSURE,
    ABS_MT_TOOL_TYPE,
];

#[repr(C)]
#[derive(Clone, Copy)]
struct InputEvent {
    tv_sec: libc::time_t,
    tv_usec: libc::suseconds_t,
    type_: u16,
    code: u16,
    value: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
struct AbsInfo {
    value: i32,
    minimum: i32,
    maximum: i32,
    fuzz: i32,
    flat: i32,
    resolution: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct InputId {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
}

#[repr(C)]
struct UinputSetup {
    id: InputId,
    name: [u8; 80],
    ff_effects_max: u32,
}

#[repr(C)]
struct UinputAbsSetup {
    code: u16,
    absinfo: AbsInfo,
}

const IOC_W: u32 = 1;
const IOC_R: u32 = 2;
fn ioc(dir: u32, ty: u8, nr: u32, size: usize) -> u32 {
    (dir << 30) | ((size as u32) << 16) | ((ty as u32) << 8) | nr
}

fn cvt(r: libc::c_int) -> io::Result<libc::c_int> {
    if r < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(r)
    }
}

unsafe fn ioctl_ptr<T>(fd: i32, req: u32, p: *mut T) -> io::Result<i32> {
    cvt(libc::ioctl(fd, req as _, p))
}

unsafe fn ioctl_int(fd: i32, req: u32, v: libc::c_int) -> io::Result<i32> {
    cvt(libc::ioctl(fd, req as _, v))
}

fn dev_name(fd: i32) -> io::Result<String> {
    let mut buf = [0u8; 256];
    unsafe { ioctl_ptr(fd, ioc(IOC_R, b'E', 0x06, buf.len()), buf.as_mut_ptr())? };
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    Ok(String::from_utf8_lossy(&buf[..end]).into_owned())
}

fn dev_id(fd: i32) -> io::Result<InputId> {
    let mut id = InputId::default();
    unsafe { ioctl_ptr(fd, ioc(IOC_R, b'E', 0x02, std::mem::size_of::<InputId>()), &mut id)? };
    Ok(id)
}

fn abs_info(fd: i32, code: u16) -> io::Result<AbsInfo> {
    let mut a = AbsInfo::default();
    unsafe {
        ioctl_ptr(fd, ioc(IOC_R, b'E', 0x40 + code as u32, std::mem::size_of::<AbsInfo>()), &mut a)?
    };
    Ok(a)
}

fn bits(fd: i32, ev: u16, nbits: usize) -> io::Result<Vec<u8>> {
    let mut b = vec![0u8; nbits / 8 + 1];
    unsafe { ioctl_ptr(fd, ioc(IOC_R, b'E', 0x20 + ev as u32, b.len()), b.as_mut_ptr())? };
    Ok(b)
}

fn bit(b: &[u8], n: usize) -> bool {
    b.get(n / 8).map_or(false, |x| x & (1 << (n % 8)) != 0)
}

fn sw_state(fd: i32) -> io::Result<Vec<u8>> {
    let mut b = vec![0u8; 8];
    unsafe { ioctl_ptr(fd, ioc(IOC_R, b'E', 0x1b, b.len()), b.as_mut_ptr())? };
    Ok(b)
}

fn mt_slots(fd: i32, code: u16, n: usize) -> io::Result<Vec<i32>> {
    let mut b = vec![0i32; n + 1];
    b[0] = code as i32;
    unsafe { ioctl_ptr(fd, ioc(IOC_R, b'E', 0x0a, b.len() * 4), b.as_mut_ptr())? };
    Ok(b[1..].to_vec())
}

fn grab(fd: i32, on: bool) -> io::Result<()> {
    unsafe { ioctl_int(fd, ioc(IOC_W, b'E', 0x90, 4), on as libc::c_int)? };
    Ok(())
}

fn open_ro(path: &str) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
}

fn find_device(name: &str) -> io::Result<Option<(String, File)>> {
    let mut paths: Vec<_> = fs::read_dir("/dev/input")?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().map_or(false, |f| f.to_string_lossy().starts_with("event")))
        .collect();
    paths.sort();
    for p in paths {
        let path = p.to_string_lossy().into_owned();
        if let Ok(f) = open_ro(&path) {
            if dev_name(f.as_raw_fd()).map_or(false, |n| n == name) {
                return Ok(Some((path, f)));
            }
        }
    }
    Ok(None)
}

fn create_uinput(src: i32, nslots: usize, name: &str) -> io::Result<File> {
    let f = OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open("/dev/uinput")?;
    let fd = f.as_raw_fd();
    let set = |nr: u32, v: i32| unsafe { ioctl_int(fd, ioc(IOC_W, b'U', nr, 4), v) };
    const UI_SET_EVBIT: u32 = 100;
    const UI_SET_KEYBIT: u32 = 101;
    const UI_SET_ABSBIT: u32 = 103;
    const UI_SET_PROPBIT: u32 = 110;

    set(UI_SET_EVBIT, EV_SYN as i32)?;
    set(UI_SET_EVBIT, EV_KEY as i32)?;
    set(UI_SET_EVBIT, EV_ABS as i32)?;

    // Mirror the real device's keys (wake gestures etc.) and add the tools.
    let keys = bits(src, EV_KEY, KEY_CNT)?;
    for k in 0..KEY_CNT {
        if bit(&keys, k) {
            set(UI_SET_KEYBIT, k as i32)?;
        }
    }
    for k in [BTN_TOUCH, BTN_TOOL_FINGER, BTN_TOOL_PEN] {
        set(UI_SET_KEYBIT, k as i32)?;
    }

    let mut axes: Vec<(u16, AbsInfo)> = vec![(
        ABS_MT_SLOT,
        AbsInfo { maximum: nslots as i32 - 1, ..Default::default() },
    )];
    for code in MT_CODES {
        let mut a = abs_info(src, code)?;
        if code == ABS_MT_TOOL_TYPE {
            a = AbsInfo { minimum: 0, maximum: MT_TOOL_PALM, ..Default::default() };
        }
        axes.push((code, a));
    }
    for (code, a) in &axes {
        set(UI_SET_ABSBIT, *code as i32)?;
        let mut s = UinputAbsSetup { code: *code, absinfo: *a };
        s.absinfo.value = 0;
        unsafe { ioctl_ptr(fd, ioc(IOC_W, b'U', 4, std::mem::size_of::<UinputAbsSetup>()), &mut s)? };
    }
    set(UI_SET_PROPBIT, INPUT_PROP_DIRECT)?;

    let src_id = dev_id(src).unwrap_or_default();
    let mut setup = UinputSetup {
        id: InputId { bustype: src_id.bustype, vendor: 0x1209, product: 0x5047, version: 1 },
        name: [0; 80],
        ff_effects_max: 0,
    };
    let nb = name.as_bytes();
    setup.name[..nb.len().min(79)].copy_from_slice(&nb[..nb.len().min(79)]);
    unsafe {
        ioctl_ptr(fd, ioc(IOC_W, b'U', 3, std::mem::size_of::<UinputSetup>()), &mut setup)?;
        ioctl_int(fd, ioc(0, b'U', 1, 0), 0)?; // UI_DEV_CREATE
    }
    Ok(f)
}

fn write_events(f: &File, evs: &[Out]) {
    if evs.is_empty() {
        return;
    }
    let buf: Vec<InputEvent> = evs
        .iter()
        .map(|e| {
            let (type_, code, value) = match *e {
                Out::Abs(c, v) => (EV_ABS, c, v),
                Out::Key(c, v) => (EV_KEY, c, v),
                Out::Syn => (EV_SYN, SYN_REPORT, 0),
            };
            InputEvent { tv_sec: 0, tv_usec: 0, type_, code, value }
        })
        .collect();
    let bytes = buf.len() * std::mem::size_of::<InputEvent>();
    let r = unsafe { libc::write(f.as_raw_fd(), buf.as_ptr() as *const libc::c_void, bytes) };
    if r < 0 {
        log(&format!("uinput write failed: {}", io::Error::last_os_error()));
    }
}

fn read_events(f: &File) -> io::Result<Vec<InputEvent>> {
    let mut buf: [InputEvent; 64] =
        [InputEvent { tv_sec: 0, tv_usec: 0, type_: 0, code: 0, value: 0 }; 64];
    let r = unsafe {
        libc::read(
            f.as_raw_fd(),
            buf.as_mut_ptr() as *mut libc::c_void,
            std::mem::size_of_val(&buf),
        )
    };
    if r < 0 {
        let e = io::Error::last_os_error();
        return if e.kind() == io::ErrorKind::WouldBlock { Ok(vec![]) } else { Err(e) };
    }
    let n = r as usize / std::mem::size_of::<InputEvent>();
    Ok(buf[..n].to_vec())
}

fn log(msg: &str) {
    use std::sync::OnceLock;
    static T0: OnceLock<Instant> = OnceLock::new();
    let t = T0.get_or_init(Instant::now).elapsed().as_millis();
    eprintln!("[{:>9}] {}", t, msg);
}

static RELOAD: AtomicBool = AtomicBool::new(false);

extern "C" fn on_sighup(_: libc::c_int) {
    RELOAD.store(true, Ordering::SeqCst);
}

/// SIGHUP re-reads the config file. Installed without SA_RESTART so a
/// blocking poll() returns EINTR and the main loop notices right away.
fn install_sighup() {
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = on_sighup as extern "C" fn(libc::c_int) as usize;
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaction(libc::SIGHUP, &sa, std::ptr::null_mut());
    }
}

/// What the app shows: daemon state plus contact sizes seen since start,
/// for tuning pen_max_major. Written to a key=value file, at most every
/// STATUS_MIN_MS while contacts are coming in.
#[derive(Default)]
struct Status {
    filtering: bool,
    pen_out: bool,
    pens: u64,
    passed: u64,
    rejected: u64,
    last_pen_major: i32,
    max_pen_major: i32,
    last_touch_major: i32,
    min_touch_major: i32,
    dirty: bool,
    written_ms: Option<u64>,
}

const STATUS_MIN_MS: u64 = 250;

impl Status {
    fn note(&mut self, n: &Note) {
        match *n {
            Note::Start { major, class: Class::Pen, .. } => {
                self.pens += 1;
                self.last_pen_major = major;
                self.max_pen_major = self.max_pen_major.max(major);
            }
            Note::Start { major, fate, .. } => {
                if fate == Fate::Reject {
                    self.rejected += 1;
                }
                self.last_touch_major = major;
                if self.min_touch_major == 0 || major < self.min_touch_major {
                    self.min_touch_major = major;
                }
            }
            Note::Promote { .. } => self.passed += 1,
            Note::Drop { .. } | Note::Cancel { .. } | Note::Regrade { .. } => self.rejected += 1,
            Note::End { .. } => return,
        }
        self.dirty = true;
    }

    /// Milliseconds until a pending write is due, if one is pending.
    fn due_in(&self, now: u64) -> Option<u64> {
        if !self.dirty {
            return None;
        }
        Some(match self.written_ms {
            Some(t) => (t + STATUS_MIN_MS).saturating_sub(now),
            None => 0,
        })
    }

    fn flush(&mut self, path: &str, now: u64) {
        if self.due_in(now) != Some(0) {
            return;
        }
        let text = format!(
            "pid={}\nfiltering={}\npen_out={}\npens={}\npassed={}\nrejected={}\n\
             last_pen_major={}\nmax_pen_major={}\nlast_touch_major={}\nmin_touch_major={}\n",
            std::process::id(),
            self.filtering as u8,
            self.pen_out as u8,
            self.pens,
            self.passed,
            self.rejected,
            self.last_pen_major,
            self.max_pen_major,
            self.last_touch_major,
            self.min_touch_major,
        );
        let tmp = format!("{}.tmp", path);
        if let Err(e) = fs::write(&tmp, text).and_then(|_| fs::rename(&tmp, path)) {
            log(&format!("can't write status {}: {}", path, e));
        }
        self.dirty = false;
        self.written_ms = Some(now);
    }
}

struct Settings {
    filter: Config,
    require_pen_out: bool,
    verbose: bool,
    touch_device: String,
    pen_device: String,
    virtual_name: String,
}

fn load_settings(path: &str) -> Settings {
    let mut s = Settings {
        filter: Config::default(),
        require_pen_out: true,
        verbose: false,
        touch_device: "goodix_ts".into(),
        pen_device: "pen_detect".into(),
        virtual_name: "palmguard_ts".into(),
    };
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => {
            log(&format!("no config at {}, using defaults", path));
            return s;
        }
    };
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((k, v)) = line.split_once('=') else { continue };
        let (k, v) = (k.trim(), v.trim());
        let num = || v.parse::<i64>().ok();
        match k {
            "pen_max_major" => s.filter.pen_max_major = num().unwrap_or(80) as i32,
            "pen_regrade_major" => s.filter.pen_regrade_major = num().unwrap_or(150) as i32,
            "grace_ms" => s.filter.grace_ms = num().unwrap_or(500) as u64,
            "holdoff_ms" => s.filter.holdoff_ms = num().unwrap_or(40) as u64,
            "require_pen_out" => s.require_pen_out = matches!(v, "1" | "true" | "yes"),
            "verbose" => s.verbose = matches!(v, "1" | "true" | "yes"),
            "touch_device" => s.touch_device = v.into(),
            "pen_device" => s.pen_device = v.into(),
            "virtual_name" => s.virtual_name = v.into(),
            _ => log(&format!("unknown config key '{}'", k)),
        }
    }
    s
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut config_path = "/data/adb/palmguard/palmguard.conf".to_string();
    let mut status_path: Option<String> = None;
    let mut verbose = false;
    let mut dry = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-c" | "--config" => {
                i += 1;
                config_path = args.get(i).cloned().unwrap_or(config_path);
            }
            "-s" | "--status" => {
                i += 1;
                status_path = args.get(i).cloned();
            }
            "-v" | "--verbose" => verbose = true,
            "-n" | "--dry-run" => {
                dry = true;
                verbose = true;
            }
            _ => {
                eprintln!("usage: palmguard [-c config] [-s status] [-v] [-n|--dry-run]");
                std::process::exit(2);
            }
        }
        i += 1;
    }

    // Status file defaults to "status" next to the config.
    let status_path = status_path.unwrap_or_else(|| {
        let dir = std::path::Path::new(&config_path).parent().unwrap_or(std::path::Path::new("."));
        dir.join("status").to_string_lossy().into_owned()
    });

    if let Err(e) = run(&config_path, &status_path, verbose, dry) {
        log(&format!("fatal: {}", e));
        std::process::exit(1);
    }
}

fn run(config_path: &str, status_path: &str, verbose_flag: bool, dry: bool) -> io::Result<()> {
    install_sighup();
    let mut st = load_settings(config_path);
    log(&format!("config: {:?}, require_pen_out={}", st.filter, st.require_pen_out));
    let mut verbose = verbose_flag || st.verbose;

    let (src_path, src) = find_device(&st.touch_device)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("no input device '{}'", st.touch_device)))?;
    let sfd = src.as_raw_fd();
    log(&format!("touchscreen: {} ({})", src_path, st.touch_device));

    let pen = find_device(&st.pen_device)?;
    let mut pen_out = match &pen {
        Some((p, f)) => {
            let docked = bit(&sw_state(f.as_raw_fd())?, SW_PEN_INSERTED as usize);
            log(&format!("pen switch: {} ({})", p, if docked { "docked" } else { "out" }));
            !docked
        }
        None => {
            log("no pen switch device found; treating the pen as always out");
            true
        }
    };

    let nslots = (abs_info(sfd, ABS_MT_SLOT)?.maximum as usize + 1).min(MAX_SLOTS);
    let mut filter = Filter::new(st.filter.clone(), nslots);
    let resync = |filter: &mut Filter| -> io::Result<()> {
        for code in MT_CODES {
            filter.seed(code, &mt_slots(sfd, code, nslots)?);
        }
        filter.seed_slot(abs_info(sfd, ABS_MT_SLOT)?.value);
        Ok(())
    };
    resync(&mut filter)?;

    let uinput = if dry {
        log("dry run: not creating a virtual device or grabbing; logging decisions only");
        None
    } else {
        let u = create_uinput(sfd, nslots, &st.virtual_name)?;
        log(&format!("created virtual touchscreen '{}'", st.virtual_name));
        Some(u)
    };

    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, -10);
    }

    let t0 = Instant::now();
    let now = || t0.elapsed().as_millis() as u64;
    let mut grabbed = false;
    let mut dropping = false;
    let mut key_pending = false;
    let mut out: Vec<Out> = Vec::with_capacity(256);
    let mut status = Status { pen_out, dirty: true, ..Default::default() };

    loop {
        if RELOAD.swap(false, Ordering::SeqCst) {
            let new = load_settings(config_path);
            if (&new.touch_device, &new.pen_device, &new.virtual_name)
                != (&st.touch_device, &st.pen_device, &st.virtual_name)
            {
                log("device names changed; restart palmguard to use them");
            }
            filter.cfg = new.filter.clone();
            st.filter = new.filter;
            st.require_pen_out = new.require_pen_out;
            verbose = verbose_flag || dry || new.verbose;
            log(&format!(
                "reloaded config: {:?}, require_pen_out={}, verbose={}",
                st.filter, st.require_pen_out, verbose
            ));
        }

        let want = !st.require_pen_out || pen_out;
        filter.set_active(want);
        if !dry && want != grabbed && filter.hw_contacts() == 0 {
            if want {
                filter.forget_output();
            }
            grab(sfd, want)?;
            grabbed = want;
            log(if grabbed { "filtering on (grabbed touchscreen)" } else { "filtering off (released touchscreen)" });
        }
        let filtering = if dry { want } else { grabbed };
        if status.filtering != filtering || status.pen_out != pen_out {
            status.filtering = filtering;
            status.pen_out = pen_out;
            status.dirty = true;
        }
        status.flush(status_path, now());

        let t = now();
        let wake = [filter.deadline().map(|d| d.saturating_sub(t)), status.due_in(t)];
        let timeout = match wake.iter().flatten().min() {
            Some(&ms) => ms.min(1000) as i32,
            None => -1,
        };
        let mut fds = vec![libc::pollfd { fd: sfd, events: libc::POLLIN, revents: 0 }];
        if let Some((_, f)) = &pen {
            fds.push(libc::pollfd { fd: f.as_raw_fd(), events: libc::POLLIN, revents: 0 });
        }
        let r = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as _, timeout) };
        if r < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(e);
        }

        out.clear();

        if fds[0].revents & (libc::POLLERR | libc::POLLHUP) != 0 {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "touchscreen went away"));
        }
        if fds[0].revents & libc::POLLIN != 0 {
            for ev in read_events(&src)? {
                if dropping {
                    if ev.type_ == EV_SYN && ev.code == SYN_REPORT {
                        dropping = false;
                        resync(&mut filter)?;
                        filter.syn(now(), &mut out);
                    }
                    continue;
                }
                match (ev.type_, ev.code) {
                    (EV_ABS, code) => filter.abs(code, ev.value),
                    (EV_SYN, SYN_REPORT) => {
                        filter.syn(now(), &mut out);
                        if key_pending && out.last() != Some(&Out::Syn) {
                            out.push(Out::Syn);
                        }
                        key_pending = false;
                    }
                    (EV_SYN, SYN_DROPPED) => {
                        dropping = true;
                        log("SYN_DROPPED, resyncing");
                    }
                    (EV_KEY, BTN_TOUCH) | (EV_KEY, BTN_TOOL_FINGER) | (EV_KEY, BTN_TOOL_PEN) => {}
                    (EV_KEY, code) => {
                        out.push(Out::Key(code, ev.value));
                        key_pending = true;
                    }
                    _ => {}
                }
            }
        }

        if let Some((_, f)) = &pen {
            if fds[1].revents & libc::POLLIN != 0 {
                for ev in read_events(f)? {
                    if ev.type_ == EV_SW && ev.code == SW_PEN_INSERTED {
                        pen_out = ev.value == 0;
                        log(if pen_out { "pen removed" } else { "pen docked" });
                    }
                }
            }
        }

        filter.tick(now(), &mut out);

        for n in filter.notes.drain(..) {
            status.note(&n);
            if verbose {
                log(&format!("{:?}", n));
            }
        }

        if grabbed {
            if let Some(u) = &uinput {
                write_events(u, &out);
            }
        }
    }
}
