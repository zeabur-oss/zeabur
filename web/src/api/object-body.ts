/** A manifest as the `object` of a `create` change: the API takes any JSON object there. */
export const objectBody = (object: object): Record<string, unknown> => ({ ...object })
