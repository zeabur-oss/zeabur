/** Rows past which a list gets a filter and a bounded frame. */
const LONG_LIST = 15

export const isLongList = (rows: number, at = LONG_LIST) => rows > at
