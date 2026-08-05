/**
 * Incremental UTF-8 decoder for chunked responses.
 *
 * WeChat `RequestTask.onChunkReceived` delivers raw ArrayBuffer slices that can
 * split a multi-byte UTF-8 character across chunks. This decoder buffers partial
 * sequences and only emits complete characters, so streaming Chinese text never
 * shows replacement characters.
 */
class Utf8Decoder {
  private pending: number[] = []

  /** Feed a chunk; returns the newly decoded text (may be empty). */
  push(bytes: Uint8Array): string {
    const out: string[] = []
    for (let i = 0; i < bytes.length; i++) {
      this.pending.push(bytes[i])
      const len = seqLength(this.pending[0])
      if (len <= 0 || this.pending.length < len) continue
      const cp = decodeSeq(this.pending, len)
      if (cp < 0) {
        // Invalid lead byte — drop and keep scanning.
        this.pending.shift()
        continue
      }
      out.push(String.fromCodePoint(cp))
      this.pending.splice(0, len)
    }
    return out.join('')
  }

  /** Decode anything left over (should normally be empty). */
  flush(): string {
    const leftover = this.pending
    this.pending = []
    if (leftover.length === 0) return ''
    // Best effort: decode as a single complete sequence.
    const cp = decodeSeq(leftover, leftover.length)
    return cp >= 0 ? String.fromCodePoint(cp) : ''
  }
}

function seqLength(first: number): number {
  if (first < 0x80) return 1
  if ((first & 0xe0) === 0xc0) return 2
  if ((first & 0xf0) === 0xe0) return 3
  if ((first & 0xf8) === 0xf0) return 4
  return -1
}

function decodeSeq(buf: number[], len: number): number {
  if (len === 1) return buf[0]
  if (len === 2) {
    if ((buf[1] & 0xc0) !== 0x80) return -1
    return ((buf[0] & 0x1f) << 6) | (buf[1] & 0x3f)
  }
  if (len === 3) {
    if ((buf[1] & 0xc0) !== 0x80 || (buf[2] & 0xc0) !== 0x80) return -1
    return ((buf[0] & 0x0f) << 12) | ((buf[1] & 0x3f) << 6) | (buf[2] & 0x3f)
  }
  if (
    (buf[1] & 0xc0) !== 0x80 ||
    (buf[2] & 0xc0) !== 0x80 ||
    (buf[3] & 0xc0) !== 0x80
  ) {
    return -1
  }
  return (
    ((buf[0] & 0x07) << 18) |
    ((buf[1] & 0x3f) << 12) |
    ((buf[2] & 0x3f) << 6) |
    (buf[3] & 0x3f)
  )
}

export { Utf8Decoder }
