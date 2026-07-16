import { ensureLogin, listMemoirs, Memoir } from '../../services/api'

Page({
  data: {
    memoirs: [] as Memoir[],
    loading: true,
    error: '',
  },

  onShow() {
    this.load()
  },

  async load() {
    this.setData({ loading: true, error: '' })
    try {
      await ensureLogin()
      const memoirs = await listMemoirs()
      this.setData({ memoirs, loading: false })
    } catch (e: any) {
      this.setData({ loading: false, error: e?.message || '加载失败' })
    }
  },

  onCreate() {
    wx.navigateTo({ url: '/pages/create/create' })
  },

  onOpenMemoir(e: WechatMiniprogram.TouchEvent) {
    const id = e.currentTarget.dataset.id as string
    wx.navigateTo({
      url: `/pages/interview/interview?memoirId=${id}&topic=${encodeURIComponent('童年与家庭')}`,
    })
  },

  onStartInterview(e: WechatMiniprogram.TouchEvent) {
    const id = e.currentTarget.dataset.id as string
    wx.navigateTo({
      url: `/pages/interview/interview?memoirId=${id}&topic=${encodeURIComponent('童年与家庭')}`,
    })
  },
})
