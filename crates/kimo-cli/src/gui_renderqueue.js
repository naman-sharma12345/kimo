// Pure render-queue view-model helper for kimo gui (v7.5): export_job_start/job_status/cancel_job already
// exist and the GUI already polls ONE export's progress inline (the Export section's own bar); this is the
// list-of-jobs layer on top -- every render started this session, each with its own live state, so a
// second or third export does not erase the last one's result before it was seen. No DOM, no new engine
// calls: this only reshapes job_status' own responses into display rows. Served at GET /gui_renderqueue.js
// so the browser and tests/js/renderqueue_test.js (Node) run the exact same bytes.
(function (root, factory) {
  if (typeof module !== 'undefined' && module.exports) { module.exports = factory(); }
  else { root.kimoRenderQueue = factory(); }
})(typeof self !== 'undefined' ? self : this, function () {
  function addJob(jobs, id, path) {
    return (jobs || []).concat([{ id: id, path: path, state: 'queued', pct: 0, note: '' }]);
  }

  // Reshapes one job_status result into the fields a queue row displays -- same numbers the single-export
  // progress bar already computes (chunks done/total, or frames done/total as the fallback), plus a short
  // note line (eta / error) instead of the single-export bar's own full sentence.
  function jobFields(s) {
    const pct = (s.chunks && s.chunks.total > 0) ? Math.round(100 * s.chunks.done / s.chunks.total)
      : (s.frames_total > 0 ? Math.round(100 * s.frames_done / s.frames_total) : 0);
    const notes = [];
    if (s.eta && s.eta.seconds != null) notes.push(`~${Math.ceil(s.eta.seconds)}s left`);
    if (s.error) notes.push(`error: ${s.error}`);
    return { state: s.state, pct: pct, note: notes.join(' \u00b7 ') };
  }
  function updateJob(jobs, id, status) {
    return (jobs || []).map((j) => (j.id === id) ? Object.assign({}, j, jobFields(status)) : j);
  }
  function removeJob(jobs, id) { return (jobs || []).filter((j) => j.id !== id); }

  function isFinished(job) { return !!job && (job.state === 'done' || job.state === 'failed' || job.state === 'cancelled'); }
  // Which jobs still need polling -- queued/running/cancelling; done/failed/cancelled are final.
  function activeJobIds(jobs) { return (jobs || []).filter((j) => !isFinished(j)).map((j) => j.id); }

  return { addJob: addJob, updateJob: updateJob, removeJob: removeJob, activeJobIds: activeJobIds, isFinished: isFinished };
});
