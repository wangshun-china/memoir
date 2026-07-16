import { createMemoir, ensureLogin } from '../../services/api'

Page({
  data: {
    subjectName: '',
    preferredName: '',
    birthYear: '',
    birthPlace: '',
    relation: '',
    submitting: false,
    error: '',
  },

  onSubject(e: WechatMiniprogram.Input) {
    this.setData({ subjectName: e.detail.value })
  },
  onPreferred(e: WechatMiniprogram.Input) {
    this.setData({ preferredName: e.detail.value })
  },
  onBirthYear(e: WechatMiniprogram.Input) {
    this.setData({ birthYear: e.detail.value })
  },
  onBirthPlace(e: WechatMiniprogram.Input) {
    this.setData({ birthPlace: e.detail.value })
  },
  onRelation(e: WechatMiniprogram.Input) {
    this.setData({ relation: e.detail.value })
  },

  async onSubmit() {
    const subject = (this.data.subjectName || '').trim()
    if (!subject) {
      this.setData({ error: '请填写回忆录主人姓名' })
      return
    }
    this.setData({ submitting: true, error: '' })
    try {
      await ensureLogin()
      const birthYearRaw = (this.data.birthYear || '').trim()
      const birth_year = birthYearRaw ? Number(birthYearRaw) : undefined
      const memoir = await createMemoir({
        subject_name: subject,
        preferred_name: (this.data.preferredName || '').trim() || undefined,
        birth_year: Number.isFinite(birth_year as number) ? birth_year : undefined,
        birth_place: (this.data.birthPlace || '').trim() || undefined,
        creator_relation: (this.data.relation || '').trim() || undefined,
      })
      wx.redirectTo({
        url: `/pages/interview/interview?memoirId=${memoir.id}&topic=${encodeURIComponent('童年与家庭')}`,
      })
    } catch (e: any) {
      this.setData({ submitting: false, error: e?.message || '创建失败' })
    }
  },
})
