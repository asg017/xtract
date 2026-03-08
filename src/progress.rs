use std::time::Instant;

use console::style;
use indicatif::{HumanDuration, MultiProgress, ProgressBar, ProgressStyle};

pub struct Progress {
    multi: MultiProgress,
    bars: Vec<ProgressBar>,
    overall: ProgressBar,
    start: Instant,
}

impl Progress {
    pub fn new(total: usize, concurrency: usize) -> Self {
        let multi = MultiProgress::new();

        let overall_style = ProgressStyle::with_template(
            "{bar:30.green/dim} {pos}/{len} pages  {elapsed_precise} ETA {eta_precise}"
        )
        .unwrap()
        .progress_chars("██░");

        let overall = multi.add(ProgressBar::new(total as u64));
        overall.set_style(overall_style);

        let spinner_style = ProgressStyle::with_template(
            "{prefix:.bold.dim} {spinner} {wide_msg}"
        )
        .unwrap()
        .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ");

        let bars: Vec<_> = (0..concurrency)
            .map(|i| {
                let pb = multi.add(ProgressBar::new_spinner());
                pb.set_style(spinner_style.clone());
                pb.set_prefix(format!("  worker {}", i + 1));
                pb.set_message("idle");
                pb
            })
            .collect();

        Self {
            multi,
            bars,
            overall,
            start: Instant::now(),
        }
    }

    /// Get a worker handle for use in a thread.
    pub fn worker(&self, index: usize) -> Worker<'_> {
        Worker {
            bar: &self.bars[index],
            overall: &self.overall,
        }
    }

    pub fn finish(&self, msg: &str) {
        for bar in &self.bars {
            bar.finish_and_clear();
        }
        self.overall.finish_and_clear();
        self.multi.clear().unwrap();
        println!(
            "{} {msg} in {}",
            style("✓").green().bold(),
            HumanDuration(self.start.elapsed()),
        );
    }

    pub fn finish_err(&self, msg: &str) {
        for bar in &self.bars {
            bar.finish_and_clear();
        }
        self.overall.finish_and_clear();
        self.multi.clear().unwrap();
        eprintln!("{} {msg}", style("✗").red().bold());
    }
}

pub struct Worker<'a> {
    bar: &'a ProgressBar,
    overall: &'a ProgressBar,
}

impl Worker<'_> {
    pub fn status(&self, page_num: u32, msg: &str) {
        self.bar.set_message(format!("p{page_num}: {msg}"));
        self.bar.tick();
    }

    pub fn complete_page(&self) {
        self.overall.inc(1);
        self.bar.set_message("idle");
    }
}
