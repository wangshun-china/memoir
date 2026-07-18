/**
 * Pure composer clear transition after a successful send.
 * Used by the interview page and by unit tests (require this file directly).
 *
 * @param {{ draft: string, composerKey: number, success: boolean }} state
 * @returns {{ draft: string, composerKey: number, shouldRemount: boolean }}
 */
function afterSuccessfulSend(state) {
  const key = typeof state.composerKey === 'number' ? state.composerKey : 0
  const draft = typeof state.draft === 'string' ? state.draft : ''
  if (!state.success) {
    return { draft, composerKey: key, shouldRemount: false }
  }
  return {
    draft: '',
    composerKey: key + 1,
    shouldRemount: true,
  }
}

module.exports = {
  afterSuccessfulSend,
}
