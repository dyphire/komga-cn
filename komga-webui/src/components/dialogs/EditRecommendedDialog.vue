<template>
  <v-dialog v-model="modal"
            max-width="450"
            :fullscreen="$vuetify.breakpoint.xsOnly"
  >
    <v-card>
      <v-card-title>{{ $t('dialog.edit_recommended.dialog_title') }}</v-card-title>
      <v-btn icon absolute top right @click="dialogClose">
        <v-icon>mdi-close</v-icon>
      </v-btn>

      <v-card-text :class="$vuetify.breakpoint.xsOnly ? 'px-0' : undefined">
        <!-- Library Selection (only for home page) -->
        <div class="mb-4">
          <v-subheader>{{ $t('dashboard.library_selector.title') }}</v-subheader>
          <v-select
            v-model="selectedLibraries"
            :items="libraryItems"
            :label="$t('dashboard.library_selector.select_libraries')"
            multiple
            chips
            deletable-chips
            :hint="$t('dashboard.library_selector.hint')"
            persistent-hint
            class="mb-2"
          >
            <template v-slot:selection="{ item, index }">
              <v-chip
                v-if="index < 2"
                small
                :close="selectedLibraries.length > 1"
                @click:close="removeLibrary(item.value)"
              >
                {{ item.text }}
              </v-chip>
              <span
                v-if="index === 2"
                class="grey--text caption"
              >
                (+{{ selectedLibraries.length - 2 }} others)
              </span>
            </template>
          </v-select>
        </div>

        <v-list>
          <draggable
            v-model="localItems"
            v-bind="dragOptions"
            handle=".handle"
          >
            <v-list-item v-for="(l, index) in localItems" :key="index">
              <v-list-item-icon>
                <v-icon class="handle">mdi-drag-horizontal-variant</v-icon>
              </v-list-item-icon>
              <v-list-item-content>
                <v-list-item-title class="handle">{{ $t(`dashboard.${l.section.toLowerCase()}`) }}</v-list-item-title>
              </v-list-item-content>
              <v-list-item-action>
                <v-switch v-model="enabled[l.section]"/>
              </v-list-item-action>
            </v-list-item>
          </draggable>
        </v-list>
      </v-card-text>

      <v-card-actions>
        <v-spacer/>
        <v-btn v-if="$vuetify.breakpoint.smAndUp" text @click="dialogClose">{{ $t('common.cancel') }}</v-btn>
        <v-btn color="error" @click="resetToDefault">{{
            $t('dialog.edit_recommended.button_reset')
          }}
        </v-btn>
        <v-btn color="primary" @click="saveChanges">{{
            $t('dialog.edit_recommended.button_confirm')
          }}
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script lang="ts">
import {UserRoles} from '@/types/enum-users'
import draggable from 'vuedraggable'
import Vue, {PropType} from 'vue'
import {
  ClientSettingsRecommendedView,
  ClientSettingsRecommendedViewSection,
  RECOMMENDED_DEFAULT,
} from '@/types/komga-clientsettings'
import {LibraryDto} from '@/types/komga-libraries'
import {LIBRARIES_ALL} from '@/types/library'


export default Vue.extend({
  name: 'EditRecommendedDialog',
  components: {draggable},
  data: function () {
    return {
      UserRoles,
      modal: false,
      localItems: [] as ClientSettingsRecommendedViewSection[],
      enabled: {} as Record<string, boolean>,
      selectedLibraries: [] as string[],
    }
  },
  props: {
    value: Boolean,
    viewConfig: {
      type: Object as PropType<ClientSettingsRecommendedView>,
      required: true,
    },
  },
  watch: {
    value(val) {
      this.modal = val
      if (val) {
        this.reset(this.viewConfig)
      }
    },
    modal(val) {
      !val && this.dialogClose()
    },
  },
  computed: {
    dragOptions(): any {
      return {
        animation: 200,
        ghostClass: 'ghost',
      }
    },
    libraries(): LibraryDto[] {
      return this.$store.getters.getLibraries
    },
    libraryItems(): { text: string; value: string }[] {
      return this.libraries.map(lib => ({
        text: lib.name,
        value: lib.id,
      }))
    },
    isIndividualLibrary(): boolean {
      // This component is used in DashboardView, so we need to check if it's showing all libraries
      return this.$route.params.libraryId !== LIBRARIES_ALL
    },
  },
  methods: {
    reset(viewConfig: ClientSettingsRecommendedView) {
      this.localItems = viewConfig?.sections || []
      this.enabled = {} as Record<string, boolean>
      this.localItems.forEach(it => this.enabled[it.section] = true)
      RECOMMENDED_DEFAULT.sections
        .filter(it => !viewConfig?.sections.some(s => s.section === it.section))
        .forEach(it => this.localItems.push(it))

      // Initialize selected libraries
      this.selectedLibraries = [...this.$store.getters.getDashboardSelectedLibraries()]
    },
    dialogClose() {
      this.$emit('input', false)
    },
    resetToDefault() {
      this.$emit('reset-defaults')
      this.dialogClose()
    },
    saveChanges() {
      const sections = this.localItems.filter(it => this.enabled[it.section])
      const updated = {
        sections: sections,
      } as ClientSettingsRecommendedView

      // Save selected libraries
      this.$store.commit('setDashboardSelectedLibraries', this.selectedLibraries)

      this.$emit('update:viewConfig', updated)
      this.dialogClose()
    },
    removeLibrary(libraryId: string) {
      const index = this.selectedLibraries.indexOf(libraryId)
      if (index >= 0) {
        this.selectedLibraries.splice(index, 1)
      }
    },
  },
})
</script>

<style scoped>
.handle {
  cursor: grab !important;
}

.ghost {
  opacity: 0.5;
  background: #c8ebfb;
}
</style>
