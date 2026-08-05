import {
  confirmStory,
  ensureLogin,
  listStories,
  organizeStories,
  StoryCard,
} from '../../services/api'

type StoryView = StoryCard & {
  confirmed: boolean
  timeLabel: string
  tagsText: string
  missingText: string
}

function buildStoryView(story: StoryCard): StoryView {
  let timeLabel = story.time_text || '时间待补充'
  if (!story.time_text && story.year_start) {
    timeLabel = story.year_end && story.year_end !== story.year_start
      ? `${story.year_start}—${story.year_end}年`
      : `${story.year_start}年`
  }
  return {
    ...story,
    confirmed: story.status === 'confirmed',
    timeLabel,
    tagsText: (story.themes || []).slice(0, 3).join(' · '),
    missingText: (story.missing_details || []).slice(0, 3).join('、'),
  }
}

Page({
  data: {
    memoirId: '',
    stories: [] as StoryView[],
    timeline: [] as StoryView[],
    loading: true,
    organizing: false,
    confirmingId: '',
    confirmedCount: 0,
    error: '',
  },

  async onLoad(query: Record<string, string | undefined>) {
    const memoirId = query.memoirId || ''
    this.setData({ memoirId })
    if (!memoirId) {
      this.setData({ loading: false, error: '缺少回忆录 ID' })
      return
    }
    await this.reload()
  },

  async onShow() {
    if (this.data.memoirId && !this.data.loading) await this.reload()
  },

  async reload() {
    try {
      this.setData({ error: '' })
      await ensureLogin()
      const stories = (await listStories(this.data.memoirId)).map(buildStoryView)
      this.setData({
        stories,
        timeline: stories.filter((story) => story.confirmed),
        confirmedCount: stories.filter((story) => story.confirmed).length,
        loading: false,
      })
    } catch (e: any) {
      this.setData({ loading: false, error: (e && e.message) || '故事箱加载失败' })
    }
  },

  onStartRandom() {
    wx.navigateTo({
      url: `/pages/interview/interview?memoirId=${this.data.memoirId}&mode=start&topic=${encodeURIComponent('自由回忆')}`,
    })
  },

  async onConfirm(e: WechatMiniprogram.TouchEvent) {
    const id = e.currentTarget.dataset.id as string
    if (!id || this.data.confirmingId) return
    this.setData({ confirmingId: id, error: '' })
    try {
      await confirmStory(id)
      wx.showToast({ title: '故事已确认', icon: 'success' })
      await this.reload()
    } catch (err: any) {
      this.setData({ error: (err && err.message) || '确认失败' })
    } finally {
      this.setData({ confirmingId: '' })
    }
  },

  async onOrganize() {
    if (this.data.organizing) return
    if (!this.data.confirmedCount) {
      wx.showToast({ title: '请先确认至少一个故事', icon: 'none' })
      return
    }
    this.setData({ organizing: true, error: '' })
    wx.showLoading({ title: '正在整理…', mask: true })
    try {
      const result = await organizeStories(this.data.memoirId)
      wx.hideLoading()
      wx.showModal({
        title: '整理完成',
        content: `已用 ${result.story_count} 个故事整理大事年谱，并生成 ${result.chapters.length} 个章节草稿。`,
        confirmText: '查看章节',
        cancelText: '留在这里',
        success: (res) => {
          if (res.confirm) {
            wx.navigateTo({
              url: `/pages/reader/reader?memoirId=${this.data.memoirId}`,
            })
          }
        },
      })
    } catch (e: any) {
      wx.hideLoading()
      this.setData({ error: (e && e.message) || '整理失败' })
    } finally {
      this.setData({ organizing: false })
    }
  },
})
