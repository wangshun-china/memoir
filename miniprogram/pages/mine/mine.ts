import {
  getMe,
  isLoggedIn,
  listMemoirs,
  logout,
  updateProfile,
  wechatLogin,
} from '../../services/api'

Page({
  data: {
    loggedIn: false,
    loggingIn: false,
    nickname: '',
    nickInitial: '我',
    avatarUrl: '',
    memoirCount: 0,
    error: '',
  },

  onShow() {
    this.refresh()
  },

  async refresh() {
    const loggedIn = isLoggedIn()
    this.setData({ loggedIn, error: '' })
    if (!loggedIn) {
      this.setData({ nickname: '', avatarUrl: '', memoirCount: 0 })
      return
    }
    try {
      const me = await getMe()
      const memoirs = await listMemoirs()
      const nickname = me.nickname || '微信用户'
      this.setData({
        nickname,
        nickInitial: nickname.charAt(0) || '我',
        avatarUrl: me.avatar_url || '',
        memoirCount: memoirs.length,
      })
    } catch (e: any) {
      this.setData({ error: e?.message || '加载失败' })
    }
  },

  async onLogin() {
    this.setData({ loggingIn: true, error: '' })
    try {
      await wechatLogin()
      this.setData({ loggingIn: false, loggedIn: true })
      await this.refresh()
    } catch (e: any) {
      this.setData({ loggingIn: false, error: e?.message || '微信登录失败' })
    }
  },

  onLogout() {
    wx.showModal({
      title: '退出登录',
      content: '确定退出当前微信账号？',
      success: (res) => {
        if (res.confirm) {
          logout()
          this.setData({
            loggedIn: false,
            nickname: '',
            avatarUrl: '',
            memoirCount: 0,
          })
        }
      },
    })
  },

  goHome() {
    wx.switchTab({ url: '/pages/home/home' })
  },

  onRefreshProfile() {
    this.refresh()
  },

  async onChooseAvatar(e: WechatMiniprogram.CustomEvent) {
    const avatarUrl = (e.detail as { avatarUrl?: string }).avatarUrl
    if (!avatarUrl) return
    this.setData({ avatarUrl })
    try {
      await updateProfile({ avatar_url: avatarUrl })
    } catch (err: any) {
      this.setData({ error: err?.message || '头像保存失败' })
    }
  },

  async onNicknameBlur(e: WechatMiniprogram.Input) {
    const nickname = (e.detail.value || '').trim()
    if (!nickname || nickname === this.data.nickname) return
    try {
      const me = await updateProfile({ nickname })
      this.setData({
        nickname: me.nickname,
        nickInitial: me.nickname.charAt(0) || '我',
      })
    } catch (err: any) {
      this.setData({ error: err?.message || '昵称保存失败' })
    }
  },
})
