//! Probe: compute the AccuracyHeatmap grid for a replay under the
//! project's current semantics vs lazer's official semantics, and print
//! both as ASCII so they can be compared with `rei_heatmap.png`.
//!
//! Project (game.rs): hits only, on circle/head; `last` = previous
//! circle/head; cursor = interpolated at judgement time.
//!
//! Official (AccuracyHeatmap.cs + ScoreProcessor): every judged object of
//! ANY type advances the `LastHitObject` chain (ticks/repeats/tails/
//! spinners too); circle/head events with ANY result are candidates, but
//! need a non-null position; position = `ClosestPressPosition` = the press
//! closest to the object centre among all presses between spawn and
//! judgement (misses included when at least one press happened).
use osu_replay_judge::engine::Engine;
use osu_replay_judge::mods::Mods;
use osu_replay_judge::process::{NestedKind, ProcKind};
use osu_replay_judge::score::HitResult;
use osu_replay_judge::{beatmap, process, replay};

const POINTS: i32 = 33;
const INNER: f32 = 0.8;
const ROTATION: f32 = 45.0;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let map_path = &args[1];
    let replay_path = &args[2];

    let content = std::fs::read_to_string(map_path).unwrap();
    let map = beatmap::decode(&content).unwrap();
    let rep = replay::decode_file(replay_path, map.version).unwrap();
    let classic = (rep.header.version as i64) < 30_000_000;
    let mods = Mods::from_legacy(rep.header.mods, classic).unwrap();
    let difficulty = process::apply_difficulty_mods(map.difficulty, mods.hard_rock, mods.easy);
    let processed = process::process(&map, difficulty, classic, mods.hard_rock);
    let mut engine = Engine::new(processed, &mods);
    engine.run(&rep.frames);

    let objects = engine.objects();
    let snaps = &engine.snapshots;
    let cs = map.difficulty.cs;
    eprintln!("cs={} objects={} timeline={}", cs, objects.len(), engine.timeline.len());

    // Press events from rising edges of the replay snapshots.
    struct Press {
        time: f64,
        pos: [f32; 2],
    }
    let mut presses: Vec<Press> = Vec::new();
    for w in rep.frames.windows(2) {
        if (w[1].left && !w[0].left) || (w[1].right && !w[0].right) {
            presses.push(Press { time: w[1].time, pos: [w[1].position.0, w[1].position.1] });
        }
    }
    eprintln!("raw frames={} presses={}", rep.frames.len(), presses.len());
    // Cursor interpolated at t (project semantics).
    let cursor_at = |t: f64| -> [f32; 2] {
        if snaps.is_empty() {
            return [256.0, 192.0];
        }
        let idx = snaps.partition_point(|s| s.time <= t).saturating_sub(1);
        let cur = snaps[idx].cursor;
        if let Some(next) = snaps.get(idx + 1) {
            let span = next.time - snaps[idx].time;
            if span > 0.0 && t > snaps[idx].time {
                let f = ((t - snaps[idx].time) / span).clamp(0.0, 1.0) as f32;
                return [cur.x + (next.cursor.x - cur.x) * f, cur.y + (next.cursor.y - cur.y) * f];
            }
        }
        [cur.x, cur.y]
    };

    // Stacked end position of the object a timeline entry refers to,
    // per label (lazer `StackedEndPosition`).
    let entry_end_pos = |obj_idx: usize, label: &str| -> [f32; 2] {
        let obj = &objects[obj_idx];
        let stack = obj.stack_offset();
        match label {
            "circle" | "head" => [obj.position.x + stack.x, obj.position.y + stack.y],
            "tail" | "slider" => [obj.end_position.x + stack.x, obj.end_position.y + stack.y],
            "spinner" | "stick" | "smax" => [obj.position.x + stack.x, obj.position.y + stack.y],
            // ticks/repeats sit on the slider path; the label carries the
            // nested index ("tick3", "repeat1").
            _ => match &obj.kind {
                ProcKind::Slider { path, nested, .. } => {
                    let prefix_len = label.find(|ch: char| ch.is_ascii_digit()).unwrap_or(label.len());
                    let idx = label[prefix_len..].parse::<usize>().unwrap_or(0);
                    let progress = nested.get(idx).map(|n| n.path_progress).unwrap_or(0.5);
                    let p = path.position_at(progress.clamp(0.0, 1.0));
                    [obj.position.x + stack.x + p.x, obj.position.y + stack.y + p.y]
                }
                _ => [obj.position.x + stack.x, obj.position.y + stack.y],
            },
        }
    };

    {
        let seq: Vec<String> = engine.timeline.iter().take(60).map(|e| format!("{}:{}:{:.0}", e.label, e.object_index, e.time)).collect();
        eprintln!("SEQ: {}", seq.join(" | "));
    }
    let radius = objects.iter().find(|o| !o.is_spinner()).map(|o| o.radius).unwrap_or(36.0) as f64;

    fn find_relative_hit_position(previous: [f32; 2], next: [f32; 2], hit: [f32; 2], radius: f64, rotation: f32) -> [f32; 2] {
        let angle1 = ((next[1] - hit[1]) as f64).atan2((hit[0] - next[0]) as f64);
        let angle2 = ((next[1] - previous[1]) as f64).atan2((previous[0] - next[0]) as f64);
        let final_angle = angle2 - angle1;
        let dist = (((hit[0] - next[0]).powi(2) + (hit[1] - next[1]).powi(2)).sqrt() as f64) / radius;
        let rotated = final_angle - rotation.to_radians() as f64;
        [-dist as f32 * rotated.cos() as f32, -dist as f32 * rotated.sin() as f32]
    }

    fn grid_from(points: &[(i32, i32, bool)]) -> (Vec<Vec<i32>>, Vec<Vec<bool>>) {
        let mut grid = vec![vec![0i32; POINTS as usize]; POINTS as usize];
        let mut miss = vec![vec![false; POINTS as usize]; POINTS as usize];
        for &(r, c, is_miss) in points {
            if r < 0 || c < 0 || r >= POINTS || c >= POINTS {
                continue;
            }
            if is_miss {
                miss[r as usize][c as usize] = true;
            } else {
                grid[r as usize][c as usize] += 1;
            }
        }
        (grid, miss)
    }

    fn print_grid(name: &str, points: &[(i32, i32, bool)]) {
        let (grid, miss) = grid_from(points);
        let peak = grid.iter().flat_map(|r| r.iter()).copied().max().unwrap_or(1).max(1);
        let total: i32 = grid.iter().flat_map(|r| r.iter()).sum();
        let miss_total = miss.iter().flatten().filter(|&&m| m).count();
        eprintln!("== {name}: {total} hit points (peak {peak}), {miss_total} miss cells");
        let centre = POINTS as f32 * 0.5;
        let mut within = 0;
        let mut rels: Vec<f32> = Vec::new();
        for r in 0..POINTS as usize {
            let mut line = String::new();
            for c in 0..POINTS as usize {
                let dx = c as f32 + 0.5 - centre;
                let dy = r as f32 + 0.5 - centre;
                let rel = (dx * dx + dy * dy).sqrt() / (centre * INNER);
                let ch = if miss[r][c] {
                    'M'
                } else if grid[r][c] == 0 {
                    '.'
                } else {
                    rels.push(rel);
                    if rel <= 0.35 {
                        within += grid[r][c];
                    }
                    let v = grid[r][c] as f32 / peak as f32;
                    char::from(b'0' + ((v * 9.0).ceil() as u8).clamp(1, 9))
                };
                line.push(ch);
            }
            eprintln!("{line}");
        }
        let mean = rels.iter().sum::<f32>() / rels.len().max(1) as f32;
        let max = rels.iter().cloned().fold(0.0f32, f32::max);
        eprintln!("   mean|rel|={mean:.3} max|rel|={max:.3} share<=0.35: {:.1}%", within as f32 / total.max(1) as f32 * 100.0);
    }

    fn dump_grid(name: &str, points: &[(i32, i32, bool)]) {
        let (grid, miss) = grid_from(points);
        eprintln!("DUMP {name}");
        for r in 0..POINTS as usize {
            let mut line = String::new();
            for c in 0..POINTS as usize {
                line.push(if miss[r][c] { 'M' } else if grid[r][c] > 0 { '#' } else { '.' });
            }
            eprintln!("{line}");
        }
    }

    fn to_cell(rel: [f32; 2]) -> (i32, i32) {
        let centre = (POINTS - 1) as f32 * 0.5;
        let local_inner = centre * INNER;
        let px = centre + local_inner * rel[0];
        let py = centre + local_inner * rel[1];
        (py.round() as i32, px.round() as i32)
    };

    // ---------------- Variant P: current project semantics ----------------
    {
        let mut last: Option<[f32; 2]> = None;
        let mut pts: Vec<(i32, i32, bool)> = Vec::new();
        let mut evrels: Vec<f32> = Vec::new();
        let mut n = 0;
        for entry in &engine.timeline {
            let obj = &objects[entry.object_index];
            let stack = obj.stack_offset();
            let pos = [obj.position.x + stack.x, obj.position.y + stack.y];
            let is_circle = entry.label == "circle" || entry.label == "head";
            if !is_circle {
                continue;
            }
            if matches!(
                entry.result,
                HitResult::Meh | HitResult::Ok | HitResult::Good | HitResult::Great | HitResult::Perfect
            ) {
                if let Some(prev) = last {
                    let hit = cursor_at(entry.time);
                    let dist = (hit[0] - pos[0]).hypot(hit[1] - pos[1]);
                    if n < 10 {
                        eprintln!("P sample t={:.0} obj=({}, {}) cursor=({}, {}) dist={:.1} units ({:.2} rel)", entry.time, pos[0], pos[1], hit[0], hit[1], dist, dist / radius as f32);
                    }
                    n += 1;
                    evrels.push(dist / radius as f32);
                    let rel = find_relative_hit_position(prev, pos, hit, radius, ROTATION);
                    let (r,c)=to_cell(rel); pts.push((r,c,false));
                }
            }
            last = Some(pos);
        }
        evrels.sort_by(|a,b| a.total_cmp(b));
        let m = evrels.iter().sum::<f32>() / evrels.len() as f32;
        let med = evrels[evrels.len()/2];
        eprintln!("PROJECT per-event: n={} mean={:.3} median={:.3} p90={:.3} max={:.3}", evrels.len(), m, med, evrels[(evrels.len() as f32 * 0.9) as usize], evrels[evrels.len()-1]);
        print_grid("PROJECT (current)", &pts);
        dump_grid("PROJ", &pts);
    }


    // ---------------- Variant F: fixed game.rs semantics ----------------
    {
        let judged_end = |obj: &osu_replay_judge::process::ProcObject, label: &str, snap_snaps: &_| -> [f32; 2] { let _ = snap_snaps; let stack = obj.stack_offset(); match label {
            "tail" | "slider" => [obj.end_position.x + stack.x, obj.end_position.y + stack.y],
            l if l.starts_with("tick") || l.starts_with("repeat") => {
                let digits = l.find(|c: char| c.is_ascii_digit()).unwrap_or(l.len());
                let idx = l[digits..].parse::<usize>().unwrap_or(0);
                match &obj.kind {
                    ProcKind::Slider { path, nested, .. } => {
                        let progress = nested.get(idx).map(|n| n.path_progress).unwrap_or(0.5);
                        let p = path.position_at(progress.clamp(0.0, 1.0));
                        [obj.position.x + stack.x + p.x, obj.position.y + stack.y + p.y]
                    }
                    _ => [obj.position.x + stack.x, obj.position.y + stack.y],
                }
            }
            _ => [obj.position.x + stack.x, obj.position.y + stack.y],
        } };
        let mut last_end: Option<[f32; 2]> = None;
        let mut pts: Vec<(i32, i32, bool)> = Vec::new();
        for entry in &engine.timeline {
            let obj = &objects[entry.object_index];
            let this_end = judged_end(obj, &entry.label, &());
            let is_circle = entry.label == "circle" || entry.label == "head";
            if is_circle && matches!(
                entry.result,
                HitResult::Meh | HitResult::Ok | HitResult::Good | HitResult::Great | HitResult::Perfect
            ) {
                if let Some(prev) = last_end {
                    let hit = cursor_at(entry.time);
                    let rel = find_relative_hit_position(prev, this_end, hit, radius, ROTATION);
                    let (r,c)=to_cell(rel); pts.push((r,c,false));
                }
            }
            last_end = Some(this_end);
        }
        dump_grid("FIX", &pts);
        print_grid("FIXED (chain fix, hits only)", &pts);
    }

    // ---------------- Variant O: official lazer semantics ----------------
    {
        let mut last: Option<[f32; 2]> = None;
        let mut proj_last: Option<[f32; 2]> = None;
        let mut chain_diff = 0usize;
        let mut chain_events = 0usize;
        let mut pts: Vec<(i32, i32, bool)> = Vec::new();
        let mut evrels: Vec<f32> = Vec::new();
        let mut dbg_n = 0;
        for entry in &engine.timeline {
            let obj = &objects[entry.object_index];
            let this_end = entry_end_pos(entry.object_index, &entry.label);
            let is_circle = entry.label == "circle" || entry.label == "head";
            if is_circle {
                if let Some(prev) = last {
                    chain_events += 1;
                    if let Some(pp) = proj_last {
                        if (prev[0] - pp[0]).hypot(prev[1] - pp[1]) > 5.0 {
                            chain_diff += 1;
                        }
                    }
                    // ClosestPressPosition: closest press to the object
                    // centre between spawn (start - preempt) and judgement.
                    let t0 = obj.start_time - obj.time_preempt;
                    let hit = presses
                        .iter()
                        .filter(|p| p.time >= t0 && p.time <= entry.time + 2.0)
                        .map(|p| {
                            let d = (p.pos[0] - this_end[0]).hypot(p.pos[1] - this_end[1]);
                            (d, p.pos)
                        })
                        .min_by(|a, b| a.0.total_cmp(&b.0))
                        .map(|(_, p)| p);
                    if dbg_n < 12 {
                        let inwin: Vec<String> = presses.iter().filter(|p| p.time >= t0 && p.time <= entry.time + 2.0).take(5).map(|p| format!("t={:.0} d={:.1}", p.time, (p.pos[0] - this_end[0]).hypot(p.pos[1] - this_end[1]))).collect();
                        eprintln!("O dbg t={:.0} obj=({}, {}) t0={:.0} nwin={:?} [{}]", entry.time, this_end[0], this_end[1], t0, presses.iter().filter(|p| p.time >= t0 && p.time <= entry.time + 2.0).count(), inwin.join(", "));
                        dbg_n += 1;
                    }
                    if let Some(hit) = hit {
                        let is_miss = !matches!(
                            entry.result,
                            HitResult::Meh | HitResult::Ok | HitResult::Good | HitResult::Great | HitResult::Perfect
                        );
                        if !is_miss {
                            let dist = (hit[0] - this_end[0]).hypot(hit[1] - this_end[1]);
                            evrels.push(dist / radius as f32);
                        }
                        let rel = find_relative_hit_position(prev, this_end, hit, radius, ROTATION);
                        let (r,c)=to_cell(rel); pts.push((r,c,is_miss));
                    }
                }
            }
            last = Some(this_end);
            let obj2 = &objects[entry.object_index];
            let is_circle2 = entry.label == "circle" || entry.label == "head";
            if is_circle2 {
                let stack2 = obj2.stack_offset();
                proj_last = Some([obj2.position.x + stack2.x, obj2.position.y + stack2.y]);
            }
        }
        eprintln!("CHAIN: {chain_diff}/{chain_events} circle events have start >5 units apart between project/official chains");
        evrels.sort_by(|a,b| a.total_cmp(b));
        let m = evrels.iter().sum::<f32>() / evrels.len() as f32;
        let med = evrels[evrels.len()/2];
        eprintln!("OFFICIAL per-event hits: n={} mean={:.3} median={:.3} p90={:.3} max={:.3}", evrels.len(), m, med, evrels[(evrels.len() as f32 * 0.9) as usize], evrels[evrels.len()-1]);
        print_grid("OFFICIAL (lazer)", &pts);
        dump_grid("OFF", &pts);
    }
}
