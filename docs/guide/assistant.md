# Assistant (offline help)

## 1. What it does

A panel at the right edge of every view (Report, Dashboard, Data, Model) that answers questions about the program from this guide book. Each answer shows the text of the best section, names every section it used, and lets you read the section or the whole note in the panel. It works offline and sends nothing anywhere. It never calculates numbers: a request for a figure is answered with the tool that makes it. A question the guide book does not cover is answered "The guide book does not cover this".

## 2. When to use it

- You forget which tool does a task, or which columns it needs.
- A message or refusal needs explaining.
- You want to find the note before you open the tool (the section's "Find the tool" button fills the Ctrl+K search).

## 3. How to use it

1. Click **Ask** at the foot of the rail, choose Help ▸ Assistant, or press F1. The same keys close it.
2. Type a question in your own words ("how do I join two tabs by a key", "what does the CL-1 summary do") and press Enter.
3. Read the answer. Click a line under "Sections used" to read that section in the panel; "Whole note" shows every section of the note.
4. "Calculate the NIM for 4,500 Cr at 9.5%" is not answered with a number. Ask "how is the NIM calculated" for the method, and run the tool on your data for the figure.

## 4. Example

Question: "How do I run a liquidity gap ladder?" The answer is the Step by step section of the gap ladder note, followed by "From: Liquidity gap ladder ▸ Step by step", with the other matching sections listed below it.

## 5. What it refuses and why

- It does not calculate, estimate or invent a figure, so that no number in your work comes from a guess.
- It does not guess when the guide book has no match; it says so.
- A local model's wording is dropped, and the guide book's own text shown with a note, when it holds a number that is in neither the guide sections nor your question.

## 6. The optional local model

The assistant works without a model. To have a model write clearer answers from the sections found (and explain general banking or financial-mathematics ideas, marked "Not from the guide book"):

1. On a PC with internet, download a small model file in GGUF format (1 to 3 GB; an instruction-tuned model of about 1 to 4 billion parameters, 4-bit) and the Windows CPU build of **llama.cpp** (a zip that holds `llama-server.exe`).
2. Copy both to the office PC by USB into `portable_data\assistant\` next to MicroParquet.exe (an unzipped llama.cpp folder may be copied in whole). Keep one model file in the folder.
3. Open the assistant, open "Local model (optional)" at the foot of the panel, click "Check again", then "Start the model". Loading a 2 GB model takes up to a minute or two. It then starts by itself when the panel opens, until you click "Stop the model".

The model program listens on this PC's own loopback address only, is started and stopped by MicroParquet, and receives only your question and the guide sections. Your data files are never given to it. The log of its start is `portable_data\assistant\model-server.log`.

## 7. Saved in

`portable_data\settings.json`: whether the panel was open, and whether the model was running. Questions and answers are not saved; they go when the program closes.

## 8. Related tools

[Ctrl+K search](README.md) for the tool itself; every note of this guide book is what the assistant reads.
