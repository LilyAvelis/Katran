# Notion search in Katran

Install the official Notion CLI and check that `ntn --version` works from
Windows. Katran also detects WinGet installations directly when its launcher
still has an older PATH.

1. Create an [internal Notion connection](https://developers.notion.com/guides/connections-and-tokens/internal-connections)
   with **Read content** enabled. Leave **Update content** and **Insert content**
   disabled.
2. In Notion, use **Connections** to grant access to two test pages. A
   connection on a parent page can also grant access to its descendants.
3. In Katran, open **Settings → Integrations → Notion**, enter the integration's
   token and save it. Katran keeps it in Windows Credential Manager for the
   current Windows user. Disconnect removes that local credential.
4. From the app that should receive the text, summon Katran with its overlay
   hotkey. Press **Ctrl+Shift+N** in Katran to go straight to Notion, or use
   **Ctrl+N** or the round **N** icon inside **Ctrl+Shift+K**. Type a page
   title and use **↓** or click a result to focus the list. **Space**
   opens or closes the page reader. **Enter** attaches a UTF-8 `.md` file in the previous window
   using Katran's file clipboard and overlay guards. The receiving app
   needs to support pasting files. **Alt+Enter** opens the selected page in
   your default browser from the results or reader.
5. **Ctrl+.** pins or unpins a selected page. **Ctrl+F** shows favorites and
   **Ctrl+H** shows the last 100 distinct previewed, opened or exported pages.
   Katran saves their IDs and titles locally and fetches content on demand.

Search requests match page titles and return up to 25 items. Notion's public
title search is not an exhaustive inventory of the pages shared with the
integration. To search page content or find a missing result, use Search in
the Notion web app. [Notion API limitations](https://developers.notion.com/reference/search-optimizations-and-limitations)

Katran does not index this
content locally. It debounces title changes by 300 ms, cancels stale jobs
and bounds each CLI request to eight seconds. The reader requests Notion
Markdown v2 when you open it. It displays standard Markdown formatting and
Notion inline styles, including bold, italics, underlining and strikethrough.
It also displays color and emoji callouts, clickable toggles, and page tables
in a horizontally scrollable grid. Embedded database and page references show
their titles and can open the corresponding Notion URL. Unsupported markup
appears as text. Katran does not load external icons or images in
the reader. The Markdown attachment uses the existing export path. Katran rejects truncated
Notion responses. The preview limit is 65,536 characters and the paste
limit is 1 MiB of Markdown.

## Browse a database in Folder

1. Open **Ctrl+Shift+N** and press the physical **\\** key to switch from pages
   to databases. Search by the data source title and press **Enter** to open it
   in Folder. Another **\\** in the picker switches back to pages.
2. **Ctrl+T** focuses the normal shelf search and filters the database by row
   title. Switch between **Gallery**, **Stream** and **Mini stream** through
   the usual Folder controls. For Notion, Mini stream shows the selected row's
   properties across the details pane. Page content loads on **Space**.
3. **Space** opens the selected page in a centered reader. **Ctrl+C** copies
   the loaded properties of the selected pages as text. Drag the selection
   to export each page as a separate `.md` file. Keep the mouse button held
   while Katran fetches the files. **Enter**
   attaches the page as `.md`, **Alt+Enter** opens Notion in the browser, and
   **Ctrl+.** pins the page in Notion favorites. Use **Ctrl+F** to view those
   favorites. **Ctrl+K** shows actions available for a Notion page. Click the
   database name in the status bar to return to the local Folder.

Rows load in groups of 30. Press **↓** at the last row or use **Load more**.
The default order comes from Notion's data source API. The standard sorting
menu supports Notion order, name, creation time and last-edited time. A named
Notion view's saved filters and sorts do not apply here. File-size sorting
and local tags are unavailable for remote pages. The same read-only
integration needs access to the source database.

Paste a database ID or a Notion page URL into the picker for a direct lookup.
When a database ID has multiple data sources, the first source opens. History
shows visited databases and pages with distinct icons. Favorites contain pages.
