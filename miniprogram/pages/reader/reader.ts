import {
  Chapter,
  ensureLogin,
  generateChapter,
  getMemoir,
  listChapters,
  deleteMemoir,
} from '../../services/api'

type ChapterView = Chapter & {
  statusLabel: string
  hasInterview: boolean
  hasDraft: boolean
  messageCount: number
  sessionId: string
}

function buildChapterView(c: Chapter): ChapterView {
  const messageCount = Number(c.message_count || 0)
  const hasDraft =
    c.has_draft === true ||
    !!(c.content && String(c.content).trim()) ||
    c.status === 'draft' ||
    c.status === 'confirmed'
  const hasInterview =
    c.has_interview === true ||
    messageCount > 0 ||
    !!(c.continue_session_id) ||
    c.status === 'collecting' ||
    hasDraft
  let statusLabel = '待采访'
  if (hasDraft) statusLabel = '已有草稿'
  else if (hasInterview) statusLabel = `采访中 · ${messageCount}条`
  return {
    ...c,
    statusLabel,
    hasInterview,
    hasDraft,
    messageCount,
    sessionId: c.continue_session_id || '',
  }
}

Page({
  data: {
    memoirId: '',
    memoirTitle: '',
    subjectName: '',
    chapters: [] as ChapterView[],
    activeId: '',
    activeContent: '',
    activeHasInterview: false,
    activeHasDraft: false,
    activeSessionId: '',
    activeTopic: '童年与家庭',
    generating: false,
    deleting: false,
    loading: true,
    error: '',
  },

  async onLoad(query: Record<string, string | undefined>) {
    const memoirId = query.memoirId || ''
    this.setData({ memoirId })
    if (!memoirId) {
      this.setData({ loading: false, error: '缺少回忆录 ID' })
      return
    }
    await this.reload(query.chapterId || '')
  },

  async onShow() {
    // Refresh progress when returning from interview/generate.
    if (this.data.memoirId && !this.data.loading) {
      await this.reload(this.data.activeId)
    }
  },

  async reload(preferChapterId?: string) {
    const memoirId = this.data.memoirId
    if (!memoirId) return
    try {
      this.setData({ loading: true, error: '' })
      await ensureLogin()
      const memoir = await getMemoir(memoirId)
      const chapters = await listChapters(memoirId, { includeContent: true })
      const views = chapters.map(buildChapterView)
      const preferredId = preferChapterId || this.data.activeId
      const preferred =
        views.find((c) => c.id === preferredId) ||
        views.find((c) => c.hasDraft) ||
        views.find((c) => c.hasInterview) ||
        views[0]
      this.applyActive(views, preferred)
      this.setData({
        memoirTitle: memoir.title,
        subjectName: memoir.subject_name,
        chapters: views,
        loading: false,
      })
    } catch (e: any) {
      this.setData({ loading: false, error: (e && e.message) || '加载失败' })
    }
  },

  applyActive(views: ChapterView[], preferred?: ChapterView) {
    const ch = preferred || views[0]
    this.setData({
      activeId: (ch && ch.id) || '',
      activeContent: (ch && ch.content) || '',
      activeHasInterview: !!(ch && ch.hasInterview),
      activeHasDraft: !!(ch && ch.hasDraft),
      activeSessionId: (ch && ch.sessionId) || '',
      activeTopic: (ch && ch.title) || '童年与家庭',
    })
  },

  onSelectChapter(e: WechatMiniprogram.TouchEvent) {
    const id = e.currentTarget.dataset.id as string
    const ch = this.data.chapters.find((c) => c.id === id)
    if (!ch) return
    this.applyActive(this.data.chapters, ch)
  },

  onGoInterview() {
    const { memoirId, activeId, activeTopic, activeSessionId, activeHasInterview } = this.data
    const topic = activeTopic || '童年与家庭'
    if (activeHasInterview && activeSessionId) {
      wx.navigateTo({
        url: `/pages/interview/interview?memoirId=${memoirId}&mode=continue&sessionId=${activeSessionId}&topic=${encodeURIComponent(topic)}&chapterId=${activeId}`,
      })
      return
    }
    if (activeHasInterview) {
      wx.navigateTo({
        url: `/pages/interview/interview?memoirId=${memoirId}&mode=continue&topic=${encodeURIComponent(topic)}`,
      })
      return
    }
    wx.navigateTo({
      url: `/pages/interview/interview?memoirId=${memoirId}&mode=start&topic=${encodeURIComponent(topic)}&chapterId=${activeId}`,
    })
  },

  async onGenerate() {
    const sessionId = this.data.activeSessionId
    if (!sessionId) {
      wx.showToast({ title: '请先完成本章采访', icon: 'none' })
      return
    }
    if (this.data.generating) return
    this.setData({ generating: true, error: '' })
    try {
      const generated = await generateChapter(sessionId)
      wx.showToast({ title: '章节已生成', icon: 'success' })
      await this.reload((generated.chapter && generated.chapter.id) || this.data.activeId)
    } catch (e: any) {
      this.setData({ error: (e && e.message) || '生成失败' })
      wx.showToast({ title: (e && e.message) || '生成失败', icon: 'none' })
    } finally {
      this.setData({ generating: false })
    }
  },

  onDeleteMemoir() {
    if (this.data.deleting) return
    wx.showModal({
      title: '删除回忆录',
      content: `确定删除「${this.data.memoirTitle || '这本回忆录'}」？采访记录与章节草稿将一并删除，且不可恢复。`,
      confirmText: '删除',
      confirmColor: '#a33',
      success: async (res) => {
        if (!res.confirm) return
        this.setData({ deleting: true })
        try {
          await deleteMemoir(this.data.memoirId)
          wx.showToast({ title: '已删除', icon: 'success' })
          setTimeout(() => {
            wx.switchTab({ url: '/pages/home/home' })
          }, 400)
        } catch (e: any) {
          this.setData({ deleting: false, error: (e && e.message) || '删除失败' })
          wx.showToast({ title: (e && e.message) || '删除失败', icon: 'none' })
        }
      },
    })
  },
})
