//! Linux parent-death signal checks.

use std::{
    io,
    os::unix::process::{CommandExt as UnixCommandExt, ExitStatusExt},
    process, thread, time,
};

use super::{CommandExt, ProcessGuard, ShutdownPolicy, Signal, arm_parent_death_signal};

/// Checks delivery after exec when the spawning thread exits but its process lives.
#[test]
fn spawning_thread_exit_signals_child() -> io::Result<()> {
    for process_group in [false, true] {
        let mut guard = thread::spawn(move || {
            let mut command = process::Command::new("sleep");
            command.arg("60").parent_death_signal(Signal::SIGUSR1);
            if process_group {
                ProcessGuard::spawn_process_group(&mut command, ShutdownPolicy::default())
            } else {
                ProcessGuard::spawn(&mut command)
            }
        })
        .join()
        .expect("spawning thread must not panic")?;

        let started = time::Instant::now();
        let status = loop {
            if let Some(status) = guard
                .child
                .as_mut()
                .expect("guard owns the child")
                .try_wait()?
            {
                break status;
            }
            assert!(
                started.elapsed() < time::Duration::from_secs(5),
                "child survived spawning thread exit"
            );
            thread::sleep(time::Duration::from_millis(10));
        };
        assert_eq!(status.signal(), Some(Signal::SIGUSR1 as i32));
    }
    Ok(())
}

/// Checks that a parent mismatch aborts spawning rather than executing the command.
#[test]
fn parent_mismatch_fails_spawn() {
    let mut command = process::Command::new("true");
    // SAFETY: The hook uses only async-signal-safe operations, as in production.
    unsafe {
        command
            .pre_exec(|| arm_parent_death_signal(Signal::SIGTERM, nix::unistd::Pid::from_raw(0)));
    }
    let error = command.spawn().expect_err("parent PID must not match zero");
    assert_eq!(error.raw_os_error(), Some(nix::errno::Errno::ESRCH as i32));
}
