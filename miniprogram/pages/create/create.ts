import { createMemoir, ensureLogin } from '../../services/api'

type FormFields = {
  subjectName: string
  preferredName: string
  birthYear: string
  birthPlace: string
  relation: string
}

Page({
  data: {
    submitting: false,
    error: '',
    relationChip: '',
  },

  _form: {
    subjectName: '',
    preferredName: '',
    birthYear: '',
    birthPlace: '',
    relation: '',
  } as FormFields,

  onSubject(e: WechatMiniprogram.Input) {
    this._form.subjectName = e.detail.value || ''
  },
  onPreferred(e: WechatMiniprogram.Input) {
    this._form.preferredName = e.detail.value || ''
  },
  onBirthYear(e: WechatMiniprogram.Input) {
    this._form.birthYear = e.detail.value || ''
  },
  onBirthPlace(e: WechatMiniprogram.Input) {
    this._form.birthPlace = e.detail.value || ''
  },
  onRelation(e: WechatMiniprogram.Input) {
    this._form.relation = e.detail.value || ''
  },

  onRelationChip(e: WechatMiniprogram.TouchEvent) {
    const v = (e.currentTarget.dataset.v as string) || ''
    this.setData({ relationChip: v })
    if (v && v !== '其他') {
      this._form.relation = v
    } else if (v === '其他') {
      this._form.relation = ''
    }
  },

  async onSubmit() {
    const subject = (this._form.subjectName || '').trim()
    if (!subject) {
      this.setData({ error: '请填写回忆录主人姓名' })
      return
    }
    this.setData({ submitting: true, error: '' })
    try {
      await ensureLogin()
      const birthYearRaw = (this._form.birthYear || '').trim()
      const birth_year = birthYearRaw ? Number(birthYearRaw) : undefined
      const memoir = await createMemoir({
        subject_name: subject,
        preferred_name: (this._form.preferredName || '').trim() || undefined,
        birth_year: Number.isFinite(birth_year as number) ? birth_year : undefined,
        birth_place: (this._form.birthPlace || '').trim() || undefined,
        creator_relation: (this._form.relation || '').trim() || undefined,
      })
      const firstChapter = memoir.chapters && memoir.chapters[0]
      const chapterQ =
        firstChapter && firstChapter.id ? '&chapterId=' + firstChapter.id : ''
      wx.redirectTo({
        url:
          '/pages/interview/interview?memoirId=' +
          memoir.id +
          '&mode=start&topic=' +
          encodeURIComponent('童年与家庭') +
          chapterQ,
      })
    } catch (e: any) {
      const msg = e && e.message ? e.message : '创建失败'
      this.setData({ submitting: false, error: msg })
    }
  },
})
