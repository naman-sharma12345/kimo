//! Showcase finding: a 1080p export died silently on the 2 GB box. The guard's hard ceiling sat above physical RAM (budget*1.25+512 MB), so the
//! kernel OOM killer fired before kimo could refuse. The ceiling must stay under physical RAM, and export workers must scale with the memory budget.
use kimo_core::guard;
use std::sync::atomic::Ordering::Relaxed;
static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn ram() -> usize { std::fs::read_to_string("/proc/meminfo").unwrap().lines().find(|l| l.starts_with("MemTotal")).and_then(|l| l.split_whitespace().nth(1).and_then(|v| v.parse::<usize>().ok())).unwrap() * 1024 }
#[test] fn hard_ceiling_is_below_physical_ram_and_threads_follow_budget() { let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("KIMO_MEM_MB", "1200"); guard::init();
    let hard = guard::HARD.load(Relaxed); assert!(hard <= ram() / 100 * 85, "hard ceiling {hard} must be <= 85% of RAM {}", ram());
    let old = 1200usize * (1 << 20); let old_hard = old + old / 4 + (512 << 20); assert!(old_hard > old, "sanity");
    assert_eq!(guard::export_threads(1920, 1080, 8), 3, "1200 MB budget / (33 MB x 12) = 3 workers at 1080p");
    assert_eq!(guard::export_threads(1280, 720, 8), 7, "720p needs less per frame");
    assert_eq!(guard::export_threads(3840, 2160, 8), 1, "4K at this budget must fall back to one worker");
    assert_eq!(guard::export_threads(1920, 1080, 2), 2, "never more workers than cpus");
}

#[test] fn oversize_export_returns_an_error_and_small_budgets_scale_down() { let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Same process, KIMO_MEM_MB read once by limits(): this test only uses the pure plan function on the budget the first test installed (1200 MB).
    std::env::set_var("KIMO_MEM_MB", "1200"); guard::init();
    match guard::export_plan(1920, 1080, 8, false) { Ok((n, _)) => assert!((1..=3).contains(&n), "workers {n}"), Err(e) => assert!(e.contains("needs about"), "{e} (box memory was low)") }
    let e = guard::export_plan(7680, 4320, 8, false).expect_err("8K cannot fit 1200 MB"); assert!(e.contains("needs about") && e.contains("KIMO_MEM_MB"), "{e}");
}
#[test] fn oversize_frame_is_an_error_not_an_abort() { let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Fallible path: an 8K frame (530 MB) and an absurd one must return Err under a small budget, and a normal frame must still allocate.
    let prev = guard::BUDGET.swap(300 << 20, Relaxed);
    let e = kimo_core::effects::Frame::try_new(7680, 4320).err().expect("8K must not fit 300 MB"); assert!(e.contains("memory budget exceeded"), "{e}");
    let e = kimo_core::effects::Frame::try_new(usize::MAX / 8, 8).err().expect("overflow must be an error"); assert!(e.contains("overflows"), "{e}");
    assert!(kimo_core::effects::Frame::try_new(320, 180).is_ok()); guard::BUDGET.store(prev, Relaxed);
}
