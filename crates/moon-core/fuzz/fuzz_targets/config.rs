#![no_main]
libfuzzer_sys::fuzz_target!(|data:&str|{let _=moon_core::config::Config::load(data);let _=moon_core::config::migrate(data);});
