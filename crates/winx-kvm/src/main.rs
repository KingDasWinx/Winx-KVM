// Esconde o console no build de release no Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    winx_kvm_lib::run();
}
