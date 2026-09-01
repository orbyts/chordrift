(function attachMaintenanceDecisions(root) {
  async function resolution(change, selected) {
    if (!selected) throw new Error(`Choose an answer for ${change.summary}.`);
    if (selected === 'keep') return { type: 'keep_observed' };
    if (selected === 'exclude') return { type: 'exclude' };
    if (selected === 'consume') {
      if (!change.current_surface) throw new Error('The saved-track source is missing from this review.');
      return { type: 'consume_intake', parameters: { source: change.current_surface } };
    }
    let destination;
    try { destination = JSON.parse(selected); } catch (_) { throw new Error('The selected destination is not part of this review.'); }
    if (!destination?.surface_id || !destination?.name) throw new Error('The selected destination is incomplete.');
    return { type: change.kind === 'removal' ? 'restore' : 'place', parameters: { destination } };
  }

  root.ChordriftMaintenance = Object.freeze({ resolution });
}(globalThis));
