import {
  ensureLogin,
  finishInterview,
  InterviewMessage,
  listMessages,
  postMessage,
  startInterview,
} from '../../services/api'

Page({
  data: {
    memoirId: '',
    sessionId: '',
    topic: '童年与家庭',
    topicHint: '小时候住过的家',
    messages: [] as InterviewMessage[],
    draft: '',
    sending: false,
    finished: false,
    error: '',
    scrollInto: '',
  },

  async onLoad(query: Record<string, string | undefined>) {
    const memoirId = query.memoirId || ''
    const topic = decodeURIComponent(query.topic || '童年与家庭')
    this.setData({
      memoirId,
      topic,
      topicHint: topic === '童年与家庭' ? '小时候住过的家' : topic,
    })
    if (!memoirId) {
      this.setData({ error: '缺少回忆录 ID' })
      return
    }
    try {
      await ensureLogin()
      const started = await startInterview(memoirId, topic)
      this.setData({ sessionId: started.session.id })
      await this.reloadMessages()
    } catch (e: any) {
      this.setData({ error: e?.message || '无法开始采访' })
    }
  },

  async reloadMessages() {
    const sessionId = this.data.sessionId
    if (!sessionId) return
    const messages = await listMessages(sessionId)
    const last = messages[messages.length - 1]
    this.setData({
      messages,
      scrollInto: last ? `m-${last.id}` : '',
    })
  },

  onDraft(e: WechatMiniprogram.Input) {
    this.setData({ draft: e.detail.value })
  },

  async onSend() {
    const content = (this.data.draft || '').trim()
    if (!content) {
      this.setData({ error: '请先输入内容' })
      return
    }
    await this.sendWithAction(content, 'normal')
    this.setData({ draft: '' })
  },

  async onAction(e: WechatMiniprogram.TouchEvent) {
    const action = e.currentTarget.dataset.action as
      | 'dont_know'
      | 'change_question'
      | 'prefer_not'
      | 'end'
    if (action === 'end') {
      await this.endSession()
      return
    }
    await this.sendWithAction('', action)
  },

  async sendWithAction(
    content: string,
    action: 'normal' | 'dont_know' | 'change_question' | 'prefer_not' | 'end',
  ) {
    if (!this.data.sessionId || this.data.finished) return
    this.setData({ sending: true, error: '' })
    try {
      const resp = await postMessage(this.data.sessionId, content, action)
      if (resp.session_status === 'finished') {
        this.setData({ finished: true })
      }
      await this.reloadMessages()
    } catch (e: any) {
      this.setData({ error: e?.message || '发送失败' })
    } finally {
      this.setData({ sending: false })
    }
  },

  async endSession() {
    if (!this.data.sessionId) return
    this.setData({ sending: true, error: '' })
    try {
      await postMessage(this.data.sessionId, '结束本次采访', 'end')
      await finishInterview(this.data.sessionId)
      this.setData({ finished: true })
      await this.reloadMessages()
    } catch (e: any) {
      this.setData({ error: e?.message || '结束失败' })
    } finally {
      this.setData({ sending: false })
    }
  },
})
