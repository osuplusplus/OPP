//! Compare the engine's computed scores against the .osr header values.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut game = osu_replay_render::game::load(&args[1], &args[2]).unwrap();
    eprintln!("final_score={} final_classic={} max_combo={}", game.final_score, game.final_classic_score, game.final_max_combo);
    eprintln!("hidden={} rate={}", game.hidden, game.rate);
}
