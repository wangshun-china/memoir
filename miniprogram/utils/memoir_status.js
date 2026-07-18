/**
 * Derive start/continue UI state from a memoirs list item.
 */

function deriveMemoirInterviewState(raw) {
  const r = raw || {}
  const message_count = Number(r.message_count)
  const count = Number.isFinite(message_count) ? message_count : 0
  const continue_session_id = r.continue_session_id
    ? String(r.continue_session_id)
    : ''
  const continue_topic = r.continue_topic
    ? String(r.continue_topic)
    : '童年与家庭'
  const has_interview =
    r.has_interview === true ||
    r.has_interview === 1 ||
    r.has_interview === 'true' ||
    count > 0 ||
    continue_session_id.length > 0

  // Soft progress for UI (message density, not chapter completion).
  // Cap visual bar at ~40 messages ≈ "丰富".
  const progress_pct = Math.max(0, Math.min(100, Math.round((count / 40) * 100)))

  let status_text = '尚未开始'
  let status_tone = 'idle'
  let primary_label = '开始采访'
  if (has_interview) {
    status_text = count > 0 ? '采访中 · 已记 ' + count + ' 条' : '已开始采访'
    status_tone = 'active'
    primary_label = '继续采访'
  }

  return {
    has_interview,
    message_count: count,
    continue_session_id,
    continue_topic,
    progress_pct,
    status_text,
    status_tone,
    primary_label,
  }
}

function normalizeMemoirList(list) {
  if (!Array.isArray(list)) return []
  return list.map(function (item) {
    const base = item && typeof item === 'object' ? item : {}
    const derived = deriveMemoirInterviewState(base)
    return Object.assign({}, base, derived)
  })
}

module.exports = {
  deriveMemoirInterviewState,
  normalizeMemoirList,
}
