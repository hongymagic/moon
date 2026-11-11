#![no_main]
libfuzzer_sys::fuzz_target!(|data:&str|{let _=moon_core::protocol::Request::parse(data);let _=data.parse::<moon_core::command::Command>();});
