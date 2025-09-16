<template>
  <v-container fluid class="pa-6">
    <v-row>
      <v-col cols="auto">
        <v-checkbox
          v-model="form.posterStretch"
          @change="$v.form.posterStretch.$touch()"
          :label="$t('ui_settings.label_poster_stretch')"
          hide-details
        />

        <v-select
          v-model="form.posterStretchMode"
          @change="$v.form?.posterStretchMode?.$touch()"
          :label="$t('ui_settings.label_poster_stretch_mode')"
          :items="stretchModeItems"
          item-text="text"
          item-value="value"
          hide-details
        />

        <v-checkbox
          v-model="form.posterBlurUnread"
          @change="$v.form.posterBlurUnread.$touch()"
          :label="$t('ui_settings.label_poster_blur_unread')"
          hide-details
        />
      </v-col>
    </v-row>
    <v-row>
      <v-col cols="auto">
        <v-btn @click="refreshSettings"
               :disabled="discardDisabled"
        >{{ $t('common.discard') }}
        </v-btn>
      </v-col>
      <v-col cols="auto">
        <v-btn color="primary"
               :disabled="saveDisabled"
               @click="saveSettings"
        >{{ $t('common.save_changes') }}
        </v-btn>
      </v-col>
    </v-row>
  </v-container>
</template>

<script lang="ts">
import Vue from 'vue'
import {CLIENT_SETTING, ClientSettingUserUpdateDto} from '@/types/komga-clientsettings'

export default Vue.extend({
  name: 'UIUserSettings',
  data: () => ({
    form: {
      posterStretch: false,
      posterStretchMode: 'top',
      posterBlurUnread: false,
    },
  }),
  validations: {
    form: {
      posterStretch: {},
      posterStretchMode: {
        required: false,
      },
      posterBlurUnread: {},
    },
  },
  mounted() {
    this.refreshSettings()
  },
  computed: {
    saveDisabled(): boolean {
      return this.$v.form.$invalid || !this.$v.form.$anyDirty
    },
    discardDisabled(): boolean {
      return !this.$v.form.$anyDirty
    },
    stretchModeItems(): Array<{text: string, value: string}> {
      return [
        { text: String(this.$t('ui_settings.stretch_mode_left')), value: 'left' },
        { text: String(this.$t('ui_settings.stretch_mode_right')), value: 'right' },
        { text: String(this.$t('ui_settings.stretch_mode_center')), value: 'top' },
      ]
    },
  },
  methods: {
    async refreshSettings() {
      await this.$store.dispatch('getClientSettingsUser')
      this.form.posterStretch = this.$store.state.komgaSettings.clientSettingsUser[CLIENT_SETTING.WEBUI_POSTER_STRETCH]?.value === 'true'
      this.form.posterStretchMode = this.$store.state.komgaSettings.clientSettingsUser[CLIENT_SETTING.WEBUI_POSTER_STRETCH_MODE]?.value || 'top'
      this.form.posterBlurUnread = this.$store.state.komgaSettings.clientSettingsUser[CLIENT_SETTING.WEBUI_POSTER_BLUR_UNREAD]?.value === 'true'
      this.$v.form.$reset()
    },
    async saveSettings() {
      let newSettings = {} as Record<string, ClientSettingUserUpdateDto>

      if (this.$v.form?.posterStretch?.$dirty)
        newSettings[CLIENT_SETTING.WEBUI_POSTER_STRETCH] = {
          value: this.form.posterStretch ? 'true' : 'false',
        }
      if (this.$v.form?.posterStretchMode?.$dirty && this.form.posterStretchMode)
        newSettings[CLIENT_SETTING.WEBUI_POSTER_STRETCH_MODE] = {
          value: this.form.posterStretchMode,
        }
      if (this.$v.form?.posterBlurUnread?.$dirty)
        newSettings[CLIENT_SETTING.WEBUI_POSTER_BLUR_UNREAD] = {
          value: this.form.posterBlurUnread ? 'true' : 'false',
        }

      await this.$komgaSettings.updateClientSettingUser(newSettings)

      await this.refreshSettings()
    },
  },
})
</script>

<style scoped>

</style>
