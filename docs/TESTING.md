# Manual validation before a release

Use disposable documents. Record Office build/bitness, Windows build, monitor
arrangement and DPI scales. Mark each case Passed, Failed or Not tested; automated
tests do not establish success for any of these cases. Current live status is
**not certified**. Never use an unsaved production document as a test fixture.

1. Install, upgrade, cancel installation, uninstall and reinstall with Office closed.
   Verify real progress and rollback, shortcuts, registration, and retained settings.
2. Start Word/Excel without documents: no empty strip over the start page.
3. Open multiple documents, switch repeatedly, reorder tabs, open and close tabs.
4. Arrange the host with Windows Snap, switch tabs and check visible placement.
   Separately record Shell Snap Group behavior; it is not guaranteed.
5. Detach and redock in both primary-to-secondary and secondary-to-primary directions.
   Check different DPI values, negative screen coordinates and maximized windows.
6. Ensure another independent window stays visible and Excel's Ribbon stays usable.
7. Close saved/unsaved documents using Save, Don't Save and Cancel. Confirm no loop,
   lost document, suppressed prompt or premature process exit. Test Close Active,
   Close All, Cancel and the persisted "don't warn" preference.
8. Verify Manager enable/disable, reset, About and Cancel. Restart Office when needed.
9. Check installer and Manager layout at 100%, 125%, 150% and 200% DPI.

Record actual evidence without publishing document content or personal paths.
Do not mark a release stable until relevant failures are resolved and retested.
