//! Replays the getevent captures from the real device through the filter.

use palmguard::filter::*;
use std::collections::{HashMap, HashSet};

fn code(name: &str) -> Option<u16> {
    Some(match name {
        "ABS_MT_SLOT" => ABS_MT_SLOT,
        "ABS_MT_TOUCH_MAJOR" => ABS_MT_TOUCH_MAJOR,
        "ABS_MT_POSITION_X" => ABS_MT_POSITION_X,
        "ABS_MT_POSITION_Y" => ABS_MT_POSITION_Y,
        "ABS_MT_TOOL_TYPE" => ABS_MT_TOOL_TYPE,
        "ABS_MT_TRACKING_ID" => ABS_MT_TRACKING_ID,
        "ABS_MT_PRESSURE" => ABS_MT_PRESSURE,
        _ => return None,
    })
}

fn value(v: &str) -> i32 {
    match v {
        "MT_TOOL_FINGER" => MT_TOOL_FINGER,
        "MT_TOOL_PEN" => MT_TOOL_PEN,
        "MT_TOOL_PALM" => MT_TOOL_PALM,
        "DOWN" => 1,
        "UP" => 0,
        h => u32::from_str_radix(h, 16).expect("hex") as i32,
    }
}

struct Result {
    notes: Vec<Note>,
    /// Virtual contacts seen in the output: id -> set of tool types used.
    out_tools: HashMap<i32, HashSet<i32>>,
    /// Output frames in which each id was lifted while not labelled palm
    /// (i.e. a real lift that Android would deliver as ACTION_UP).
    clean_lifts: HashSet<i32>,
}

fn replay(file: &str) -> Result {
    let text = std::fs::read_to_string(format!("{}/tests/data/{}", env!("CARGO_MANIFEST_DIR"), file)).unwrap();
    let mut f = Filter::new(Config::default(), 10);
    let mut notes = vec![];
    let mut out = vec![];
    let mut t0: Option<f64> = None;
    for line in text.lines() {
        let Some((ts, rest)) = line.trim_start_matches('[').split_once(']') else { continue };
        let t: f64 = ts.trim().parse().unwrap();
        let t0 = *t0.get_or_insert(t);
        let now = ((t - t0) * 1000.0).round() as u64;
        let tok: Vec<&str> = rest.split_whitespace().collect();
        if tok.len() < 3 {
            continue;
        }
        f.tick(now, &mut out);
        match tok[0] {
            "EV_ABS" => {
                if let Some(c) = code(tok[1]) {
                    f.abs(c, value(tok[2]));
                }
            }
            "EV_SYN" if tok[1] == "SYN_REPORT" => f.syn(now, &mut out),
            _ => {}
        }
        notes.append(&mut f.notes);
    }

    // Reconstruct the virtual device from the output stream.
    let mut slot = 0usize;
    let mut ids = [-1i32; MAX_SLOTS];
    let mut tools = [0i32; MAX_SLOTS];
    let mut out_tools: HashMap<i32, HashSet<i32>> = HashMap::new();
    let mut clean_lifts = HashSet::new();
    let mut touching = 0;
    for e in &out {
        match *e {
            Out::Abs(ABS_MT_SLOT, v) => slot = v as usize,
            Out::Abs(ABS_MT_TRACKING_ID, -1) => {
                if tools[slot] != MT_TOOL_PALM {
                    clean_lifts.insert(ids[slot]);
                }
                ids[slot] = -1;
            }
            Out::Abs(ABS_MT_TRACKING_ID, v) => ids[slot] = v,
            Out::Abs(ABS_MT_TOOL_TYPE, v) => {
                tools[slot] = v;
                out_tools.entry(ids[slot]).or_default().insert(v);
            }
            Out::Key(BTN_TOUCH, v) => touching = v,
            _ => {}
        }
    }
    if f.hw_contacts() == 0 {
        assert!(ids.iter().all(|&i| i == -1), "virtual contacts left down: {:?}", ids);
        assert_eq!(touching, 0, "BTN_TOUCH left down");
    }
    for n in &notes {
        println!("{:?}", n);
    }
    Result { notes, out_tools, clean_lifts }
}

fn start_of(r: &Result, id: i32) -> (Class, Fate) {
    r.notes
        .iter()
        .find_map(|n| match n {
            Note::Start { id: i, class, fate, .. } if *i == id => Some((*class, *fate)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no start for {}", id))
}

fn end_of(r: &Result, id: i32) -> Fate {
    r.notes
        .iter()
        .find_map(|n| match n {
            Note::End { id: i, fate, .. } if *i == id => Some(*fate),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no end for {}", id))
}

#[test]
fn pen_alone_is_a_stylus() {
    let r = replay("pen.txt");
    let id = 0x6cb2;
    assert_eq!(start_of(&r, id), (Class::Pen, Fate::Pass));
    assert_eq!(r.out_tools[&id], HashSet::from([MT_TOOL_PEN]));
    assert!(r.clean_lifts.contains(&id));
}

#[test]
fn finger_alone_passes_after_holdoff() {
    let r = replay("finger.txt");
    let id = 0x6cb3;
    assert_eq!(start_of(&r, id), (Class::Touch, Fate::Pending));
    assert_eq!(end_of(&r, id), Fate::Pass);
    assert_eq!(r.out_tools[&id], HashSet::from([MT_TOOL_FINGER]));
    assert!(r.clean_lifts.contains(&id));
}

#[test]
fn full_palm_never_reaches_apps() {
    let r = replay("palm.txt");
    assert!(r.out_tools.is_empty(), "palm leaked: {:?}", r.out_tools);
}

#[test]
fn writing_with_hand_resting() {
    let r = replay("both.txt");
    // Every pen stroke comes through as a stylus.
    for id in [28122, 28126, 28128, 28131, 28132, 28133, 28135] {
        assert_eq!(start_of(&r, id).0, Class::Pen, "stroke {}", id);
        assert_eq!(r.out_tools[&id], HashSet::from([MT_TOOL_PEN]), "stroke {}", id);
    }
    // Hand edge landing 25 ms before the pen: held back, then dropped.
    assert_eq!(start_of(&r, 28121), (Class::Touch, Fate::Pending));
    assert_eq!(end_of(&r, 28121), Fate::Reject);
    // Side of hand resting while writing, and the flickers around pen lifts.
    for id in [28127, 28129, 28130, 28134] {
        assert_eq!(start_of(&r, id), (Class::Touch, Fate::Reject), "contact {}", id);
    }
    // Nothing that isn't the pen is ever delivered as a normal touch-up.
    for id in [28121, 28124, 28125, 28127, 28129, 28130, 28134] {
        assert!(!r.clean_lifts.contains(&id), "contact {} leaked as a tap", id);
    }
    // The one hand contact that did get through (well after the pen lifted)
    // was flagged palm by the firmware, so it's cancelled, not tapped.
    assert!(!r.clean_lifts.contains(&28123));
}
