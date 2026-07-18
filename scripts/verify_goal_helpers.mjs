/**
 * Committed unit checks for shipped pure helpers used by the miniprogram.
 * Run: node scripts/verify_goal_helpers.mjs
 */
import { createRequire } from 'module'
import { fileURLToPath } from 'url'
import path from 'path'

const require = createRequire(import.meta.url)
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const { afterSuccessfulSend } = require(path.join(root, 'miniprogram/utils/composer_clear.js'))
const { deriveMemoirInterviewState, normalizeMemoirList } = require(
  path.join(root, 'miniprogram/utils/memoir_status.js'),
)

function assert(cond, msg) {
  if (!cond) throw new Error(msg)
}

// --- clear after successful send ---
const failPath = afterSuccessfulSend({ draft: '未发送内容', composerKey: 2, success: false })
assert(failPath.draft === '未发送内容', 'failed send must keep draft')
assert(failPath.composerKey === 2, 'failed send must not bump key')
assert(failPath.shouldRemount === false, 'failed send must not remount')

const okPath = afterSuccessfulSend({ draft: '上一句中文回答', composerKey: 2, success: true })
assert(okPath.draft === '', 'success must clear draft')
assert(okPath.composerKey === 3, 'success must remount key +1')
assert(okPath.shouldRemount === true, 'success must remount')

// --- home interview status ---
assert(
  deriveMemoirInterviewState({ has_interview: false, message_count: 0 }).has_interview === false,
  'empty memoir stays not-started',
)
assert(
  deriveMemoirInterviewState({ has_interview: false, message_count: 4 }).has_interview === true,
  'message_count alone marks started',
)
assert(
  deriveMemoirInterviewState({ continue_session_id: 'abc', message_count: 0 }).has_interview === true,
  'continue_session_id alone marks started',
)
const normalized = normalizeMemoirList([{ id: 'm1', message_count: 2, has_interview: false }])
assert(normalized[0].has_interview === true, 'normalize flips has_interview from message_count')
assert(normalized[0].message_count === 2, 'normalize keeps count')

console.log(
  JSON.stringify(
    {
      ok: true,
      clearAfterSend: okPath,
      memoirStatus: {
        empty: deriveMemoirInterviewState({ message_count: 0 }),
        withMessages: deriveMemoirInterviewState({ message_count: 4 }),
        normalized: normalized[0],
      },
    },
    null,
    2,
  ),
)
