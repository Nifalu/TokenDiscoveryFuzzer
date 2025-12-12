use core::time::Duration;
use std::{env, path::PathBuf};
mod utils;
mod config;
mod smart_token_mutations;
mod extractors;
mod token_discovery_stage;
mod token_preserving_scheduled_mutator;
mod processors;

use libafl::{
    corpus::{Corpus, InMemoryCorpus, /*InMemoryOnDiskCorpus,*/ OnDiskCorpus},
    events::{EventConfig, launcher::Launcher},
    executors::{inprocess::InProcessExecutor, ExitKind},
    feedback_or, feedback_or_fast,
    feedbacks::{CrashFeedback, MaxMapFeedback, TimeFeedback, TimeoutFeedback},
    fuzzer::{Fuzzer, StdFuzzer},
    inputs::{BytesInput, HasTargetBytes},
    monitors::{MultiMonitor, PrometheusMonitor},
    mutators::{havoc_mutations::havoc_mutations, HavocScheduledMutator},
    observers::{CanTrack, HitcountsMapObserver, StdMapObserver, TimeObserver},
    schedulers::{
        powersched::PowerSchedule, IndexesLenTimeMinimizerScheduler, StdWeightedScheduler,
    },
    stages::{calibrate::CalibrationStage, mutational::StdMutationalStage},
    state::{HasCorpus, StdState},
    Error, HasMetadata,
};

use libafl_bolts::{
    rands::StdRand,
    tuples::{tuple_list, Merge, Handled},
    core_affinity::Cores,
    shmem::StdShMemProvider,
    AsSlice,
};

use libafl_targets::{libfuzzer_initialize, libfuzzer_test_one_input, EDGES_MAP, MAX_EDGES_FOUND};
use mimalloc::MiMalloc;

use crate::config::{config, ExtractorConfig, FuzzerPreset, SchedulerPreset};
use crate::extractors::{Extractor, CorpusExtractor, MutationDeltaExtractor};
use crate::processors::build_pipeline;
use crate::smart_token_mutations::{SmartTokenInsert, SmartTokenReplace, SmartTokens};
use crate::token_discovery_stage::TokenDiscoveryStage;
use crate::token_preserving_scheduled_mutator::TokenPreservingScheduledMutator;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[no_mangle]
pub extern "C" fn libafl_main() {
    let cfg = config();
    println!("Workdir: {:?}", env::current_dir().unwrap().to_string_lossy());
    println!(
        "Config: preset={:?}, scheduler={:?}",
        cfg.fuzzer_preset, cfg.scheduler_preset
    );

    fuzz(
        &[PathBuf::from(&cfg.corpus_dir)],
        PathBuf::from(&cfg.crashes_dir),
        cfg.broker_port
    )
        .expect("An error occurred while fuzzing");
}

fn fuzz(corpus_dirs: &[PathBuf], objective_dir: PathBuf, broker_port: u16) -> Result<(), Error> {
    let cfg = config();

    let prometheus_address = format!("{}:{}", cfg.prometheus_host, cfg.prometheus_port);
    let mon = PrometheusMonitor::new(prometheus_address, |s| log::info!("{s}"));
    let multi = MultiMonitor::new(move |s| {
        if !cfg.silent_run && !cfg.disable_multimonitor {
            println!("{s}"); // only print if not in silent mode and multimonitor is not disabled
        }
    });
    let monitor = tuple_list!(mon, multi);

    let cores = Cores::from_cmdline(&cfg.cores)?;

    let corpus_dirs_clone = corpus_dirs.to_vec();
    let objective_dir_clone = objective_dir.clone();

    let _ = Launcher::builder()
        .configuration(EventConfig::AlwaysUnique)
        .monitor(monitor)
        .run_client(move |state: Option<_>, mut restarting_mgr, _core_id| {
            let cfg = config();
            let corpus_dirs = &corpus_dirs_clone;
            let objective_dir = &objective_dir_clone;

            #[allow(static_mut_refs)]
            let edges_observer = unsafe {
                HitcountsMapObserver::new(StdMapObserver::from_mut_ptr(
                    "edges",
                    EDGES_MAP.as_mut_ptr(),
                    MAX_EDGES_FOUND,
                ))
            };
            let edges_observer = edges_observer.track_indices();
            let edges_handle = edges_observer.handle();
            let time_observer = TimeObserver::new("time");

            let map_feedback = MaxMapFeedback::new(&edges_observer);
            let calibration = CalibrationStage::new(&map_feedback);

            let mut feedback = feedback_or!(map_feedback, TimeFeedback::new(&time_observer));
            let mut objective = feedback_or_fast!(CrashFeedback::new(), TimeoutFeedback::new());

            let mut state = state.unwrap_or_else(|| {
                StdState::new(
                    StdRand::new(),
                    InMemoryCorpus::<BytesInput>::new(),
                    OnDiskCorpus::new(objective_dir).unwrap(),
                    &mut feedback,
                    &mut objective,
                )
                    .unwrap()
            });

            if state.metadata_map().get::<SmartTokens>().is_none() {
                state.add_metadata(SmartTokens::new());
            }

            let power = match cfg.scheduler_preset {
                SchedulerPreset::Fast => PowerSchedule::fast(),
                SchedulerPreset::Explore => PowerSchedule::explore(),
                SchedulerPreset::Exploit => PowerSchedule::exploit(),
                SchedulerPreset::Coe => PowerSchedule::coe(),
                SchedulerPreset::Lin => PowerSchedule::lin(),
                SchedulerPreset::Quad => PowerSchedule::quad(),
            };

            let scheduler = IndexesLenTimeMinimizerScheduler::new(
                &edges_observer,
                StdWeightedScheduler::with_schedule(&mut state, &edges_observer, Some(power)),
            );

            let mut fuzzer = StdFuzzer::new(scheduler, feedback, objective);

            let mut harness = |input: &BytesInput| {
                let target = input.target_bytes();
                let buf = target.as_slice();
                unsafe {
                    libfuzzer_test_one_input(buf);
                }
                ExitKind::Ok
            };

            let mut executor = InProcessExecutor::with_timeout(
                &mut harness,
                tuple_list!(edges_observer, time_observer),
                &mut fuzzer,
                &mut state,
                &mut restarting_mgr,
                Duration::new(cfg.timeout_secs, 0),
            )?;

            let args: Vec<String> = env::args().collect();
            if unsafe { libfuzzer_initialize(&args) } == -1 {
                println!("Warning: LLVMFuzzerInitialize failed with -1");
            }

            if state.must_load_initial_inputs() {
                state
                    .load_initial_inputs(&mut fuzzer, &mut executor, &mut restarting_mgr, corpus_dirs)
                    .unwrap_or_else(|_| panic!("Failed to load initial corpus at {:?}", corpus_dirs));
                println!("Imported {} inputs from disk.", state.corpus().count());
            }

            match cfg.fuzzer_preset {
                FuzzerPreset::Baseline => {
                    let mutator = HavocScheduledMutator::new(havoc_mutations());
                    let mut stages = tuple_list!(calibration, StdMutationalStage::new(mutator));
                    fuzzer.fuzz_loop(&mut stages, &mut executor, &mut state, &mut restarting_mgr)?;
                }

                FuzzerPreset::StandardTokens => {
                    let mutations = havoc_mutations().merge(tuple_list!(
                        SmartTokenInsert::new(),
                        SmartTokenReplace::new(),
                    ));
                    let mutator = HavocScheduledMutator::new(mutations);
                    let mutational = StdMutationalStage::new(mutator);

                    let extractor = match &cfg.extractor {
                        ExtractorConfig::Corpus => Extractor::Corpus(CorpusExtractor),
                        ExtractorConfig::MutationDelta => Extractor::MutationDelta(
                            MutationDeltaExtractor::new(edges_handle.clone())
                        ),
                    };
                    let processors = build_pipeline(&cfg.pipeline);
                    let discovery = TokenDiscoveryStage::new(extractor, processors);

                    let mut stages = tuple_list!(calibration, mutational, discovery);
                    fuzzer.fuzz_loop(&mut stages, &mut executor, &mut state, &mut restarting_mgr)?;
                }

                FuzzerPreset::PreservingTokens => {
                    let mutations = havoc_mutations().merge(tuple_list!(
                        SmartTokenInsert::new(),
                        SmartTokenReplace::new(),
                    ));
                    let mutator = TokenPreservingScheduledMutator::new(mutations);
                    let mutational = StdMutationalStage::new(mutator);

                    let extractor = match &cfg.extractor {
                        ExtractorConfig::Corpus => Extractor::Corpus(CorpusExtractor),
                        ExtractorConfig::MutationDelta => Extractor::MutationDelta(
                            MutationDeltaExtractor::new(edges_handle.clone())
                        ),
                    };
                    let processors = build_pipeline(&cfg.pipeline);
                    let discovery = TokenDiscoveryStage::new(extractor, processors);

                    let mut stages = tuple_list!(calibration, mutational, discovery);
                    fuzzer.fuzz_loop_for(&mut stages, &mut executor, &mut state, &mut restarting_mgr, 10_000_000)?;
                }
            }

            Ok(())
        })
        .cores(&cores)
        .broker_port(broker_port)
        .shmem_provider(StdShMemProvider::default())
        .build()
        .launch();

    Ok(())
}