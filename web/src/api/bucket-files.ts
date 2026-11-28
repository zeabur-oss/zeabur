// The files of a bucket in an object store
// (`/api/v1/projects/{project}/object-storage/{store}/buckets/{bucket}`).
import { infiniteQueryOptions, queryOptions } from "@tanstack/react-query"

import * as zeabur from "@/api/zeabur"
import { queryClient } from "@/api/queries"

/** The bucket the calls go to: its project, its store, and its S3 name. */
export interface BucketPlace {
  project: string
  store: string
  bucket: string
}

const bucketKey = (p: BucketPlace) => ["project", p.project, "bucket-files", p.store, p.bucket]

/** One folder, a page of keys at a time. */
export const listingQuery = (p: BucketPlace, prefix: string) =>
  infiniteQueryOptions({
    queryKey: [...bucketKey(p), "list", prefix],
    queryFn: async ({ pageParam, signal }) =>
      await zeabur.listBucketObjects(
        p.project,
        p.store,
        p.bucket,
        { prefix: prefix || undefined, after: pageParam },
        { signal },
      ),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (page) => page.next ?? undefined,
    staleTime: 5_000,
  })

export const bucketPreviewQuery = (p: BucketPlace, key: string) =>
  queryOptions({
    queryKey: [...bucketKey(p), "preview", key],
    queryFn: async ({ signal }) =>
      await zeabur.previewBucketObject(p.project, p.store, p.bucket, { key }, { signal }),
    staleTime: 0,
  })

export const countQuery = (p: BucketPlace, prefix: string) =>
  queryOptions({
    queryKey: [...bucketKey(p), "count", prefix],
    queryFn: async ({ signal }) =>
      await zeabur.countBucketObjects(p.project, p.store, p.bucket, { prefix }, { signal }),
    staleTime: 0,
  })

/** Marks every read of the bucket as out of date. */
export const refreshBucket = (p: BucketPlace) =>
  queryClient.invalidateQueries({ queryKey: bucketKey(p) })

/** Where an object downloads from, or shows in the page with `inline`. */
export const objectUrl = (p: BucketPlace, key: string, inline?: boolean) =>
  zeabur.getDownloadBucketObjectUrl(p.project, p.store, p.bucket, {
    key,
    ...(inline ? { inline } : {}),
  })
