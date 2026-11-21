# Privacy: when Rolidium uses the network

Rolidium works without the network. It has **two optional functions** that connect to `rolidium.com`, and nothing else does. Everything else (modelling, files, `rolidium cli`,
`rolidium mcp`, the self test, the benchmark, saving a report or diagnostics to a file) never touches the network. All addresses are in one place in the code, `crates/rolidium-net/src/lib.rs`: only `https`
and only the domain `rolidium.com`, the certificate is always checked, a redirect is never followed, and every call has a time limit.

## 1. The question about a new version

*When.* Only if you said **Yes** on the small card shown at the first start (or switched *Check for a new version once a day* on in Settings). Until you answer, the program does not connect to the network.
At most once a day, in the background, with a limit of 5 seconds; the start of the program never waits for it and a network error is not shown to you (it goes to the log).

*What is sent.* One `GET` request:

```
GET https://rolidium.com/api/version.json?v=0.2.0&os=linux&osv=debian%2013&arch=x86_64
User-Agent: Rolidium/0.2.0
```

* `v`: the version of the program; `os`: `linux` or `windows`; `osv`: the name and the **main number** of the version of the system (`debian 13`, `windows 10`), nothing more detailed; `arch`: the processor type.
* Nothing else: no identifier of you or of the computer, no cookies, no data about the hardware, no file names. (Like any web request, it reaches the server with the IP address of your connection, which the
  server sees; the program adds nothing to it.) Settings shows the exact text of the question.

*The answer.* A JSON object with `latest` (a version) and optionally `released` (a date). It is limited to 16 kB and read strictly: a damaged or too long answer is only written to the log. If `latest` is higher
than the running version, a small notice says *Version X is available* with the buttons **Download** and **Skip this version**. *Download* opens the fixed page `https://rolidium.com/download` in your browser; the
address is never taken from the answer of the server. The program downloads and starts nothing by itself.

*How to switch it off.* Answer **No** on the card, or switch it off in Settings. `ROLIDIUM_OFFLINE=1` in the environment switches off all network use of the program whatever the settings say.

## 2. Reporting a bug or an idea (with diagnostics, if you wish)

*When.* Only when you ask: the folded row **Report a bug or an idea** at the bottom of the right column opens a panel. Nothing is sent by itself. After a run that ended with a crash the panel opens by itself at the
next start with a note that the program ended badly; it still waits for you to press Send.

*What you decide.* Bug or Idea, a text of up to 1000 characters, an optional e-mail address for a reply, and whether the diagnostics are attached (on by default; an idea can be sent without them). The link
**Show what is sent** lists every file of the zip, and each can be opened. **Nothing is sent before you press Send.**

*What is in the zip.* First a readable **summary** (`summary.txt`): the version and the commit of the program, the language; the graphics card (name, manufacturer, driver and its version), the interface used
(DirectX 12, Vulkan or OpenGL), whether the picture is drawn by software, whether the interface was chosen automatically or forced, and which interfaces are blocked after a crash; the processor (model, physical and
logical cores), the number of threads set and in use, the memory, the name and version of the system, the architecture, the size of the window and the scale. Then the log of the program, its settings (no folders, no
file names, no recent files), details of all graphics adapters and, when the size allows, small test pictures drawn by the graphics backends (a grid, axes and a box, not your model). **No models and no names of
models, no name of the computer, no user name, no serial numbers, no network addresses.** The home folder is replaced by `~` and the names of model files (`.rolidium`, `.step`, `.stp`, `.stl`) by `<model file>`.
The zip is at most 2 MB.

*How it is sent.* A `POST` (multipart) to `https://rolidium.com/api/diagnostics` with the fields `note` (your text), `kind` (`bug` or `idea`), `contact` (only if you typed one), `file` (the zip, only if attached),
`v` (version) and `os`. The answer carries a report number, which is shown to you.

*If the server does not answer or refuses* (the server may not exist yet), your text stays in the panel, and it offers to save the report to a file (a zip; your text is in it as the file `message.txt`) and to send it
by e-mail to connect@rolid.com. With `ROLIDIUM_OFFLINE=1` the panel offers only saving to a file.

You can also save the diagnostics alone to a file at any time (*About* or *Settings*, **Save diagnostics**, or `rolidium --diagnose`) and send them yourself.

## Links

Web https://rolidium.com, support forum https://rolidium.com/forum, download https://rolidium.com/download, this notice https://rolidium.com/privacy. The About window and the README link to the web and
the forum; they open in the browser of the system when you click them.
