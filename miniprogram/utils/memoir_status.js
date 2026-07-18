/**
 * Derive start/continue UI state from a memoirs list item (API may omit fields
 * if an older server is running). Pure function — used by home page + unit tests.
 *
 * @param {Record<string, unknown>} raw
 * @returns {{
 *   has_interview: boolean,
 *   message_count: number,
 *   continue_session_id: string,
 *   continue_topic: string,
 * }}
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
  // True if server flag says so, or any message/session id is present.
  const has_interview =
    r.has_interview === true ||
    r.has_interview === 1 ||
    r.has_interview === 'true' ||
    count > 0 ||
    continue_session_id.length > 0
  return {
    has_interview,
    message_count: count,
    continue_session_id,
    continue_topic,
  }
}

/**
 * Map raw list payload into cards the home UI can bind safely.
 * @param {unknown[]} list
 */
function normalizeMemoirList(list) {
  if (!Array.isArray(list)) return []
  return list.map((item) => {
    const base = item && typeof item === 'object' ? item : {}
    const derived = deriveMemoirInterviewState(base)
    return Object.assign({}, base, derived)
  })
}

module.exports = {
  deriveMemoirInterviewState,
  normalizeMemoirList,
}
