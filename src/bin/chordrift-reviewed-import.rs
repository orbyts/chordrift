//! Host-local planner for an account-owner-reviewed history import.

use std::{env, fs, process::ExitCode};

use chordrift::{config, db, sync_plan};
use serde::Deserialize;

#[derive(Deserialize)]
struct CadencePlan {
    tracks: usize,
    playlists: Vec<CadencePlaylist>,
}

#[derive(Deserialize)]
struct CadencePlaylist {
    playlist: String,
    final_order: Vec<CadenceTrack>,
}

#[derive(Deserialize)]
struct CadenceTrack {
    spotify_id: String,
    #[serde(default)]
    historical_spotify_id: Option<String>,
    track: String,
    artists: String,
    plays: i64,
    source: String,
    position: i32,
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> chordrift::Result<()> {
    let mut arguments = env::args().skip(1);
    let account = arguments.next().ok_or_else(usage)?;
    let input = arguments.next().ok_or_else(usage)?;
    if arguments.next().is_some() {
        return Err(usage());
    }
    let cadence: CadencePlan = serde_json::from_slice(&fs::read(input)?)?;
    let playlist_count = cadence.playlists.len();
    let additions = cadence
        .playlists
        .into_iter()
        .flat_map(|playlist| {
            playlist
                .final_order
                .into_iter()
                .filter(|track| track.source == "restored")
                .map(move |track| sync_plan::ReviewedAdditionInput {
                    playlist_name: playlist.playlist.clone(),
                    spotify_track_id: track.spotify_id,
                    historical_spotify_track_id: track.historical_spotify_id,
                    title: track.track,
                    artists: track.artists,
                    final_position: track.position,
                    play_count: track.plays,
                })
        })
        .collect::<Vec<_>>();
    if cadence.tracks != additions.len() {
        return Err(chordrift::ChordriftError::Configuration(format!(
            "cadence plan declares {} reviewed tracks but contains {} restored rows",
            cadence.tracks,
            additions.len()
        )));
    }
    let database = db::connect(config::database_config_from_env()?).await?;
    db::require_schema_through(&database, 51).await?;
    let report = sync_plan::create_reviewed_addition_plan(&database, &account, &additions).await?;
    println!("plan_id: {}", report.plan_id);
    println!("tracks: {}", report.operation_count);
    println!("playlists: {playlist_count}");
    println!("input_hash: {}", report.input_hash);
    println!("reused: {}", report.reused);
    println!("spotify_writes: disabled");
    Ok(())
}

fn usage() -> chordrift::ChordriftError {
    chordrift::ChordriftError::Configuration(
        "usage: chordrift-reviewed-import ACCOUNT CADENCE_PLAN_JSON".to_owned(),
    )
}
