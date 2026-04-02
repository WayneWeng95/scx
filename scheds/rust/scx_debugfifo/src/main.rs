// Copyright (c) 2024
//
// This software may be used and distributed according to the terms of the
// GNU General Public License version 2.

//! # Debug FIFO Scheduler
//!
//! A simple FIFO scheduler built on `scx_rustland_core` that prints all data
//! available from the scx framework: per-task info on every dequeue/dispatch,
//! and global scheduler statistics every second.

mod bpf_skel;
pub use bpf_skel::*;
pub mod bpf_intf;

#[rustfmt::skip]
mod bpf;
use std::mem::MaybeUninit;
use std::time::SystemTime;

use anyhow::Result;
use bpf::*;
use libbpf_rs::OpenObject;
use scx_utils::libbpf_clap_opts::LibbpfOpts;
use scx_utils::UserExitInfo;

const SLICE_NS: u64 = 5_000_000;

struct Scheduler<'a> {
    bpf: BpfScheduler<'a>,
    total_dispatched: u64,
}

impl<'a> Scheduler<'a> {
    fn init(open_object: &'a mut MaybeUninit<OpenObject>) -> Result<Self> {
        let open_opts = LibbpfOpts::default();
        let bpf = BpfScheduler::init(
            open_object,
            open_opts.clone().into_bpf_open_opts(),
            0,           // exit_dump_len
            false,       // partial
            false,       // debug
            true,        // builtin_idle
            false,       // numa_local
            SLICE_NS,    // default time slice
            "debugfifo", // scx ops name
        )?;
        Ok(Self {
            bpf,
            total_dispatched: 0,
        })
    }

    fn dispatch_tasks(&mut self) {
        let nr_waiting = *self.bpf.nr_queued_mut();

        while let Ok(Some(task)) = self.bpf.dequeue_task() {
            // Print all QueuedTask fields
            let comm = task.comm_str();
            println!("--- DEQUEUE task ---");
            println!("  pid:             {}", task.pid);
            println!("  comm:            {}", comm);
            println!("  cpu (prev):      {}", task.cpu);
            println!("  nr_cpus_allowed: {}", task.nr_cpus_allowed);
            println!("  flags:           {:#x}", task.flags);
            println!("  start_ts:        {} ns", task.start_ts);
            println!("  stop_ts:         {} ns", task.stop_ts);
            println!("  exec_runtime:    {} ns ({:.3} ms)", task.exec_runtime, task.exec_runtime as f64 / 1_000_000.0);
            println!("  weight:          {}", task.weight);
            println!("  vtime:           {}", task.vtime);
            println!("  enq_cnt:         {}", task.enq_cnt);

            // Build dispatched task
            let mut dispatched_task = DispatchedTask::new(&task);

            // Select CPU - FIFO: use idle CPU if available, otherwise any
            let cpu = self.bpf.select_cpu(task.pid, task.cpu, task.flags);
            dispatched_task.cpu = if cpu >= 0 { cpu } else { RL_CPU_ANY };

            // Simple FIFO: fixed slice divided by queue pressure
            dispatched_task.slice_ns = SLICE_NS / (nr_waiting + 1);

            // Print all DispatchedTask fields
            println!("  => DISPATCH:");
            println!("     target cpu:   {}", if dispatched_task.cpu == RL_CPU_ANY { "ANY".to_string() } else { dispatched_task.cpu.to_string() });
            println!("     slice_ns:     {} ns ({:.3} ms)", dispatched_task.slice_ns, dispatched_task.slice_ns as f64 / 1_000_000.0);
            println!("     flags:        {:#x}", dispatched_task.flags);
            println!("     vtime:        {}", dispatched_task.vtime);
            println!("     enq_cnt:      {}", dispatched_task.enq_cnt);
            println!();

            self.bpf.dispatch_task(&dispatched_task).unwrap();
            self.total_dispatched += 1;
        }

        // Notify BPF that dispatching is complete; this sleeps until more work arrives.
        self.bpf.notify_complete(0);
    }

    fn print_stats(&mut self) {
        let nr_online_cpus     = *self.bpf.nr_online_cpus_mut();
        let nr_running         = *self.bpf.nr_running_mut();
        let nr_queued          = *self.bpf.nr_queued_mut();
        let nr_scheduled       = *self.bpf.nr_scheduled_mut();
        let nr_user_dispatches = *self.bpf.nr_user_dispatches_mut();
        let nr_kernel_dispatches = *self.bpf.nr_kernel_dispatches_mut();
        let nr_cancel_dispatches = *self.bpf.nr_cancel_dispatches_mut();
        let nr_bounce_dispatches = *self.bpf.nr_bounce_dispatches_mut();
        let nr_failed_dispatches = *self.bpf.nr_failed_dispatches_mut();
        let nr_sched_congested   = *self.bpf.nr_sched_congested_mut();

        println!("========== SCHEDULER STATS ==========");
        println!("  online_cpus:        {}", nr_online_cpus);
        println!("  running tasks:      {}", nr_running);
        println!("  queued tasks:       {}", nr_queued);
        println!("  scheduled tasks:    {}", nr_scheduled);
        println!("  user dispatches:    {}", nr_user_dispatches);
        println!("  kernel dispatches:  {}", nr_kernel_dispatches);
        println!("  cancel dispatches:  {}", nr_cancel_dispatches);
        println!("  bounce dispatches:  {}", nr_bounce_dispatches);
        println!("  failed dispatches:  {}", nr_failed_dispatches);
        println!("  sched congested:    {}", nr_sched_congested);
        println!("  total dispatched (this session): {}", self.total_dispatched);
        println!("=====================================");
        println!();
    }

    fn now() -> u64 {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    fn run(&mut self) -> Result<UserExitInfo> {
        let mut prev_ts = Self::now();

        while !self.bpf.exited() {
            self.dispatch_tasks();

            let curr_ts = Self::now();
            if curr_ts > prev_ts {
                self.print_stats();
                prev_ts = curr_ts;
            }
        }
        self.bpf.shutdown_and_report()
    }
}

fn main() -> Result<()> {
    println!(
        r#"
=======================================================================
  scx_debugfifo - Debug FIFO Scheduler
  Prints ALL data from the scx framework on every scheduling event.
  NOT for production use.
=======================================================================
"#
    );

    let mut open_object = MaybeUninit::uninit();
    loop {
        let mut sched = Scheduler::init(&mut open_object)?;
        if !sched.run()?.should_restart() {
            break;
        }
    }

    Ok(())
}
