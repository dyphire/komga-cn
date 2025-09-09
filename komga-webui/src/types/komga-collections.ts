export interface CollectionDto {
  id: string,
  name: string,
  ordered: boolean,
  filtered: boolean,
  seriesIds: string[],
  createdDate: Date,
  lastModifiedDate: Date
}

export interface CollectionCreationDto {
  name: string,
  ordered: boolean,
  seriesIds: string[]
}

export interface CollectionUpdateDto {
  name?: string,
  ordered?: boolean,
  seriesIds?: string[]
}

export interface CollectionThumbnailDto {
  id: string,
  collectionId: string,
  type: string,
  selected: boolean,
  mediaType: string,
  fileSize: number,
  width: number,
  height: number,
}
