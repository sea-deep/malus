use malus::engine::{
    discovery::select_best_browser, process::BrowserProcess, profile::ProfileManager,
};
use std::{
    fs,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
#[ignore = "requires Chromium; account tests also require malus --login"]
fn test_pr_set_pdeathsig_terminates_browser_on_hard_kill() {
    let base_temp = std::env::temp_dir().join(format!("malus_pdeathsig_{}", std::process::id()));
    let pid_file = base_temp.join("browser.pid");

    if let Ok(helper_val) = std::env::var("MALUS_PDEATHSIG_HELPER") {
        let helper_dir = std::path::PathBuf::from(helper_val);
        let profile = ProfileManager::with_custom_path(&helper_dir);
        let candidate = select_best_browser(None).expect("No browser candidate found");

        let browser = BrowserProcess::spawn(&candidate, profile, false, "about:blank")
            .expect("Failed to spawn browser");
        let pid = browser.pid().expect("Browser must have PID");

        // Write PID to communication file
        let _ = fs::write(helper_dir.join("browser.pid"), pid.to_string());

        // Leak the browser so Drop is NOT called
        std::mem::forget(browser);

        // Keep process running until parent kills it
        std::thread::sleep(Duration::from_secs(60));
        return;
    }

    let _ = fs::create_dir_all(&base_temp);

    // Parent mode: Launch helper sub-process
    let current_exe = std::env::current_exe().expect("Failed to get current_exe");
    let mut child = Command::new(current_exe)
        .arg("test_pr_set_pdeathsig_terminates_browser_on_hard_kill")
        .arg("--nocapture")
        .arg("--include-ignored")
        .env("MALUS_PDEATHSIG_HELPER", base_temp.to_str().unwrap())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("Failed to spawn helper process");

    // Wait for helper to write browser PID
    let start = Instant::now();
    let mut browser_pid: Option<u32> = None;
    while start.elapsed() < Duration::from_secs(10) {
        if let Ok(content) = fs::read_to_string(&pid_file) {
            if let Ok(pid) = content.trim().parse::<u32>() {
                browser_pid = Some(pid);
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    let b_pid = browser_pid.expect("Did not receive browser PID from helper");

    // Helper process is currently running. HARD KILL it with SIGKILL (-9).
    // This bypasses any application-level cleanup (Drop) and strictly verifies
    // the Linux kernel's PR_SET_PDEATHSIG mechanism.
    unsafe {
        libc::kill(child.id() as i32, libc::SIGKILL);
    }
    let _ = child.wait();

    // Verify that the browser process is terminated by the Linux kernel within 3 seconds
    let verify_start = Instant::now();
    let mut terminated = false;

    while verify_start.elapsed() < Duration::from_secs(3) {
        let alive = unsafe {
            let res = libc::kill(b_pid as i32, 0);
            res == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
        };

        if !alive {
            terminated = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    let _ = fs::remove_dir_all(&base_temp);

    assert!(
        terminated,
        "Browser process (PID {}) was not terminated by kernel PR_SET_PDEATHSIG after parent was SIGKILLed!",
        b_pid
    );
}
