<template>
  <div>
    <v-menu offset-y v-model="menuState">
      <template v-slot:activator="{ on }">
        <v-btn icon v-on="on" @click.prevent="">
          <v-icon>mdi-dots-vertical</v-icon>
        </v-btn>
      </template>
      <v-list dense>
        <v-list-item @click="analyze" v-if="isAdmin">
          <v-list-item-title>{{ $t('menu.analyze') }}</v-list-item-title>
        </v-list-item>
        <v-list-item @click="refreshMetadata" v-if="isAdmin">
          <v-list-item-title>{{ $t('menu.refresh_metadata') }}</v-list-item-title>
        </v-list-item>
        <v-list-item @click="addToReadList" v-if="isAdmin">
          <v-list-item-title>{{ $t('menu.add_to_readlist') }}</v-list-item-title>
        </v-list-item>
        <v-list-item @click="markRead" v-if="!isRead">
          <v-list-item-title>{{ $t('menu.mark_read') }}</v-list-item-title>
        </v-list-item>
        <v-list-item @click="markUnread" v-if="!isUnread">
          <v-list-item-title>{{ $t('menu.mark_unread') }}</v-list-item-title>
        </v-list-item>
        <v-list-item :href="fileUrl" v-if="canDownload">
          <v-list-item-title>{{ $t('browse_book.download_file') }}</v-list-item-title>
        </v-list-item>
        <v-list-item @click="promptOpenPageAPI">
          <v-list-item-title>{{ $t('browse_book.open_page_prompt') }}</v-list-item-title>
        </v-list-item>
        <v-list-item @click="promptDeleteBook" class="list-danger" v-if="isAdmin">
          <v-list-item-title>{{ $t('menu.delete') }}</v-list-item-title>
        </v-list-item>
      </v-list>
    </v-menu>
  </div>
</template>
<script lang="ts">
import {getReadProgress} from '@/functions/book-progress'
import {bookFileUrl, bookPageUrl} from '@/functions/urls'
import {ReadStatus} from '@/types/enum-books'
import Vue from 'vue'
import {BookDto, ReadProgressUpdateDto} from '@/types/komga-books'

export default Vue.extend({
  name: 'BookActionsMenu',
  data: () => {
    return {
      menuState: false,
    }
  },
  props: {
    book: {
      type: Object as () => BookDto,
      required: true,
    },
    menu: {
      type: Boolean,
      default: false,
    },
  },
  watch: {
    menuState (val) {
      this.$emit('update:menu', val)
    },
  },
  computed: {
    isAdmin (): boolean {
      return this.$store.getters.meAdmin
    },
    isRead (): boolean {
      return getReadProgress(this.book) === ReadStatus.READ
    },
    isUnread (): boolean {
      return getReadProgress(this.book) === ReadStatus.UNREAD
    },
    unavailable (): boolean {
      return this.book.deleted || this.$store.getters.getLibraryById(this.book.libraryId).unavailable
    },
    canDownload (): boolean {
      return this.$store.getters.meFileDownload && !this.unavailable
    },
    fileUrl (): string {
      return bookFileUrl(this.book.id)
    },
  },
  methods: {
    analyze () {
      this.$komgaBooks.analyzeBook(this.book)
    },
    refreshMetadata () {
      this.$komgaBooks.refreshMetadata(this.book)
    },
    addToReadList () {
      this.$store.dispatch('dialogAddBooksToReadList', [this.book.id])
    },
    async markRead () {
      const readProgress = { completed: true } as ReadProgressUpdateDto
      await this.$komgaBooks.updateReadProgress(this.book.id, readProgress)
    },
    async markUnread () {
      await this.$komgaBooks.deleteReadProgress(this.book.id)
    },
    promptDeleteBook () {
      this.$store.dispatch('dialogDeleteBook', this.book)
    },
    promptOpenPageAPI() {
      const inputData = prompt('Enter page number to open', '1')

      if (typeof inputData === 'string' && inputData.trim().length > 0) {
        const pageNumber = Number.parseInt(inputData)

        if (Number.isNaN(pageNumber)) {
          alert('Invalid page number')
          return
        } else {
          // use window.open _blank to open in new tab
          if (this.book.media.pagesCount < pageNumber) {
            alert('Page number exceeds total pages')
            return
          } else if (pageNumber < 1) {
            alert('Page number cannot be less than 1')
            return
          } else {
            window.open(bookPageUrl(this.book.id, pageNumber), '_blank', 'noopener')
          }
        }
      }
    },
  },
})
</script>
