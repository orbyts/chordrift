//! Host-local promotion of an account-owner-reviewed placement and cadence.

use std::{collections::HashMap, env, fs, process::ExitCode};

use chordrift::{config, db, proposals};
use serde::Deserialize;
use sqlx::Row;

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
    source: String,
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
    let reviewed_tracks = cadence
        .playlists
        .iter()
        .flat_map(|playlist| playlist.final_order.iter())
        .filter(|track| track.source == "restored")
        .count();
    if cadence.tracks != reviewed_tracks {
        return Err(chordrift::ChordriftError::Configuration(format!(
            "cadence plan declares {} reviewed tracks but contains {reviewed_tracks} restored rows",
            cadence.tracks
        )));
    }

    let database = db::connect(config::database_config_from_env()?).await?;
    db::require_schema_through(&database, 51).await?;
    let generation = proposals::fork_approved_for_maintenance(&database, &account).await?;
    let destinations = sqlx::query(
        "SELECT lower(playlist.name) AS name, concept.stable_key
         FROM playlists playlist
         JOIN playlist_concepts concept ON concept.id = playlist.concept_id
         WHERE playlist.generation_id = $1",
    )
    .bind(generation.generation_id)
    .fetch_all(database.pool())
    .await?
    .into_iter()
    .map(|row| {
        Ok((
            row.try_get::<String, _>("name")?,
            row.try_get::<String, _>("stable_key")?,
        ))
    })
    .collect::<chordrift::Result<HashMap<_, _>>>()?;

    let mut assigned = 0usize;
    let mut ordered = 0usize;
    for playlist in &cadence.playlists {
        let stable_key = destinations
            .get(&playlist.playlist.to_lowercase())
            .ok_or_else(|| {
                chordrift::ChordriftError::Configuration(format!(
                    "reviewed destination {:?} is not in the editable proposal",
                    playlist.playlist
                ))
            })?;
        let restored = playlist
            .final_order
            .iter()
            .filter(|track| track.source == "restored")
            .map(|track| track.spotify_id.clone())
            .collect::<Vec<_>>();
        assigned += proposals::assign_many(
            &database,
            &account,
            &restored,
            stable_key,
            "Account-owner-reviewed historical placement import",
        )
        .await?
        .len();
        let exact_order = playlist
            .final_order
            .iter()
            .map(|track| track.spotify_id.clone())
            .collect::<Vec<_>>();
        ordered += proposals::set_reviewed_order(&database, &account, stable_key, &exact_order)
            .await?
            .track_count;
    }
    let approved = proposals::approve(&database, &account, generation.generation_id).await?;
    println!("generation_id: {}", approved.generation_id);
    println!("playlists: {}", cadence.playlists.len());
    println!("assigned_tracks: {assigned}");
    println!("ordered_tracks: {ordered}");
    println!("status: {}", approved.state);
    println!("spotify_writes: disabled");
    Ok(())
}

fn usage() -> chordrift::ChordriftError {
    chordrift::ChordriftError::Configuration(
        "usage: chordrift-reviewed-cadence ACCOUNT CADENCE_PLAN_JSON".to_owned(),
    )
}
