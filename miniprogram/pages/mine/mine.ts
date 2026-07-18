import {
  getMe,
  isLoggedIn,
  listMemoirs,
  logout,
  passwordLogin,
  resetPassword,
  updateProfile,
  wechatLogin,
} from '../../services/api'

Page({
  data: {
    loggedIn: false,
    loggingIn: false,
    accountLoggingIn: false,
    accountUser: '',
    accountPass: '',
    showForgot: false,
    recoveryKey: '',
    newPass: '',
    resetting: false,
    nickname: '',
    nickInitial: '我',
    avatarUrl: '',
    username: '',
    isAdmin: false,
    memoirCount: 0,
    editingNick: false,
    nickSeed: '',
    error: '',
  },

  // 编辑中草稿，不走 setData，避免中文输入法被重置
  _nickDraft: '',

  onShow() {
    this.refresh()
  },

  async refresh() {
    const loggedIn = isLoggedIn()
    this.setData({ loggedIn, error: '', editingNick: false })
    if (!loggedIn) {
      this.setData({
        nickname: '',
        avatarUrl: '',
        username: '',
        isAdmin: false,
        memoirCount: 0,
      })
      return
    }
    try {
      const me = await getMe()
      const memoirs = await listMemoirs()
      const nickname = me.nickname || me.username || '用户'
      this._nickDraft = nickname
      this.setData({
        nickname,
        nickInitial: nickname.charAt(0) || '我',
        avatarUrl: me.avatar_url || '',
        username: me.username || '',
        isAdmin: !!me.is_admin,
        memoirCount: memoirs.length,
      })
    } catch (e: any) {
      this.setData({ error: (e && e.message) || '加载失败' })
    }
  },

  onAccountUser(e: WechatMiniprogram.Input) {
    this.setData({ accountUser: e.detail.value || '' })
  },

  onAccountPass(e: WechatMiniprogram.Input) {
    this.setData({ accountPass: e.detail.value || '' })
  },

  onRecoveryKey(e: WechatMiniprogram.Input) {
    this.setData({ recoveryKey: e.detail.value || '' })
  },

  onNewPass(e: WechatMiniprogram.Input) {
    this.setData({ newPass: e.detail.value || '' })
  },

  onToggleForgot() {
    this.setData({
      showForgot: !this.data.showForgot,
      error: '',
      recoveryKey: '',
      newPass: '',
    })
  },

  async onResetPassword() {
    const username = (this.data.accountUser || '').trim()
    const recoveryKey = this.data.recoveryKey || ''
    const newPass = this.data.newPass || ''
    if (!username || !recoveryKey || !newPass) {
      this.setData({ error: '请填写账号、恢复密钥和新密码' })
      return
    }
    this.setData({ resetting: true, error: '' })
    try {
      await resetPassword(username, recoveryKey, newPass)
      this.setData({
        resetting: false,
        showForgot: false,
        accountPass: newPass,
        recoveryKey: '',
        newPass: '',
      })
      wx.showToast({ title: '密码已重置', icon: 'success' })
    } catch (e: any) {
      this.setData({
        resetting: false,
        error: (e && e.message) || '重置失败',
      })
    }
  },

  async onAccountLogin() {
    const username = (this.data.accountUser || '').trim()
    const password = this.data.accountPass || ''
    if (!username || !password) {
      this.setData({ error: '请填写账号和密码' })
      return
    }
    this.setData({ accountLoggingIn: true, error: '' })
    try {
      const auth = await passwordLogin(username, password)
      this.setData({
        accountLoggingIn: false,
        loggedIn: true,
        accountPass: '',
        isAdmin: !!auth.is_admin,
      })
      if (auth.registered) {
        wx.showToast({ title: '已自动注册', icon: 'success' })
      }
      await this.refresh()
    } catch (e: any) {
      this.setData({
        accountLoggingIn: false,
        error: (e && e.message) || '登录失败',
      })
    }
  },

  async onLogin() {
    this.setData({ loggingIn: true, error: '' })
    try {
      await wechatLogin()
      this.setData({ loggingIn: false, loggedIn: true })
      await this.refresh()
    } catch (e: any) {
      this.setData({ loggingIn: false, error: (e && e.message) || '微信登录失败' })
    }
  },

  onAdmin() {
    wx.navigateTo({ url: '/pages/admin/admin' })
  },

  onLogout() {
    wx.showModal({
      title: '退出登录',
      content: '确定退出当前账号？',
      success: (res) => {
        if (res.confirm) {
          logout()
          this.setData({
            loggedIn: false,
            nickname: '',
            avatarUrl: '',
            username: '',
            isAdmin: false,
            memoirCount: 0,
            editingNick: false,
          })
        }
      },
    })
  },

  goHome() {
    wx.switchTab({ url: '/pages/home/home' })
  },

  onShowHelp() {
    wx.showModal({
      title: '怎么使用',
      content:
        '1. 在首页创建回忆录，填主人姓名\n2. 和采访者一问一答慢慢说\n3. 说得差不多时点「生成回忆录」\n4. 在「查看正文」阅读整理好的章节',
      showCancel: false,
      confirmText: '知道了',
    })
  },

  async onChooseAvatar(e: WechatMiniprogram.CustomEvent) {
    const avatarUrl = (e.detail as { avatarUrl?: string }).avatarUrl
    if (!avatarUrl) return
    this.setData({ avatarUrl })
    try {
      await updateProfile({ avatar_url: avatarUrl })
    } catch (err: any) {
      this.setData({ error: (err && err.message) || '头像保存失败' })
    }
  },

  onStartEditNick() {
    const seed = this.data.nickname || ''
    this._nickDraft = seed
    // 只在挂载编辑框时 set 一次 nickSeed；输入中不再 setData
    this.setData({ editingNick: true, nickSeed: seed, error: '' })
  },

  onNicknameInput(e: WechatMiniprogram.Input) {
    // 禁止 setData：受控 value 回写会打断拼音组合并切回英文
    this._nickDraft = e.detail.value || ''
  },

  async onNicknameBlur(e: WechatMiniprogram.Input) {
    const nickname = ((this._nickDraft != null ? this._nickDraft : (e.detail.value != null ? e.detail.value : ''))).trim()
    // 先卸掉编辑框，再异步保存，避免输入过程 setData
    this.setData({ editingNick: false, nickSeed: '' })
    if (!nickname || nickname === this.data.nickname) return
    try {
      const me = await updateProfile({ nickname })
      this._nickDraft = me.nickname
      this.setData({
        nickname: me.nickname,
        nickInitial: me.nickname.charAt(0) || '我',
      })
    } catch (err: any) {
      this.setData({ error: (err && err.message) || '昵称保存失败' })
    }
  },
})
