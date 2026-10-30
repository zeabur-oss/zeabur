# English

<!-- This file quotes the patterns it describes. -->
<!-- vale ZeaburEn.Please = NO -->
<!-- vale ZeaburEn.Filler = NO -->
<!-- vale ZeaburEn.Tell = NO -->
<!-- vale ZeaburEn.ServesAs = NO -->
<!-- vale ZeaburEn.Dashes = NO -->

Natural US English (*catalog*, *color*), sentence case, contractions where they fit, serial comma. Write to the reader as *you*, in the present tense, with the doer as the subject.

## Sentences

- **Doer as subject, action as verb.** "The cluster rejects this change", not "This change will be rejected". Passive only when the doer is unknown or does not matter.
- **Present tense for what the product does.** "Services with one replica are briefly interrupted", not "will have a brief interruption".
- **Verbs, not nominalisations.** "validates", not "performs validation of"; "decide", not "make a decision".
- **One idea per sentence.** Split a sentence that carries a second condition in a parenthesis or a dash.
- **Old information first, new at the end.** Start from what the reader already has (the page, the command they ran), end on the new fact.
- **Translating from Chinese:** rebuild the sentence. Chinese topic-first order ("Compose files are converted to services only in the interface") becomes an English subject and verb ("Zeabur converts Compose files to services in the interface; nothing is stored in the cluster").

## Words

- Cut *please*, *simply*, *just*, *easily*, *in order to*, *via*, *utilize*, and needless *there is* / *you can*.
- *is*, not *serves as* / *stands as*: "Each crate's README is also its crate-level documentation."
- No *we* in the interface. "Unable to load services", not "We couldn't load your services".
- Plain words for inflated ones: *use* (leverage), *show* (showcase), *important* (crucial, pivotal), *full* (comprehensive), *solid* or the actual property (robust).

## Punctuation

- **No em-dash chains.** One aside per sentence at most; prefer commas, parentheses or a second sentence. This is a house rule: Google and Microsoft allow unspaced em dashes.

  | Before | After |
  |---|---|
  | The registry token … identifies the repository by name—the project name—not by the namespace UID. | The registry token … identifies the repository by the project name, not by the namespace UID. |
  | …connections outside the list—including package updates and external API calls—will fail after you apply the changes. | After you apply the changes, connections outside the list fail, including package updates and external API calls. |
- A single … character for progress text ("Saving…"). No ellipsis in a button that does not open a further step.
- No period on labels, buttons, titles or table headers; full sentences end with one.

## Examples

| Before | After |
|---|---|
| Something changed after you reviewed this. Please check it again. | Something changed after you reviewed this. Review it again. |
| Please try again in a few minutes. | Try again in a few minutes. |
| Anyone who can edit those config files can change the settings—usually a cluster administrator. | Anyone who can edit those config files, usually a cluster administrator, can change the settings. |
| Each crate has a README that also serves as its crate-level documentation. | Each crate's README is also its crate-level documentation. |
