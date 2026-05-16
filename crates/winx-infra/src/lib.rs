//! Adapters concretos do Winx-KVM.
//!
//! Cada arquivo implementa uma ou mais ports definidas em
//! `winx-application::ports`. Esta é a única camada autorizada a importar
//! crates de I/O (`mdns-sd`, `quinn`, `windows`, `cpal`, etc).
//!
//! Adapters serão adicionados sprint a sprint conforme [docs/PLANNING.md][p].
//!
//! [p]: ../../../../../docs/PLANNING.md

// Esta crate é a ÚNICA do workspace que pode usar `unsafe` (Win32 hooks, FFI
// para drivers de áudio). Cada módulo que precisar deve declarar
// `#![allow(unsafe_code)]` localmente e justificar com SAFETY comments.
#![deny(unsafe_op_in_unsafe_fn)]

// Adapters virão aqui:
// pub mod identity_store_toml;
// pub mod secret_store_keyring;
// pub mod discovery_mdns;
// pub mod transport_quic;
// pub mod input_win32;
// pub mod monitor_win32;
// pub mod audio_cpal;
// pub mod clipboard_arboard;
// pub mod clipboard_files_win32;
// pub mod filesystem_std;
