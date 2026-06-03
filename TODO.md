# ***Bugs***

[x] Regression session not persisting between runs

[x] Regression Spaces no longer list even joined Rooms

[x] After Logout can't Login again

[x] Sync error messages stays even after resolving issue

[x] Date divider without any messages in between

[x] Latest message from Room no longer shows up

[x] When you have a Room open and you switch Space its name changes from actual to "Room"

[x] New messages listed in Room switcher should be stopped at '\n'

[x] In QR Code Login QR code doesn't work

[x] OIDC or QR Code Login can't be cancelled

[x] Search option is available in Login screen

# ***General***

[x] Remove the surrounding bubble from around Space avatars

[x] Fluent for localization

[x] Implement real time audio connections (currently delegates to Element Call via browser)

[x] Video connections using MatrixRTC (currently delegates to Element Call via browser)

[x] Way to enlarge images

[x] Implement every Setting (autoloading images, typing notifications etc...)

[x] Logging in via QR code

[x] Refactor code into smaller functions moved into smaller files

[x] Location sharing in chat editor

[x] Multi-line editing

[x] One unified error info display mechanism

[x] Room with only two users (DM) should show other participants avatar

# ***Search***

[x] Add search icon and bar to header

[ ] Context aware search

[x] Filter out Rooms in Space settings

[x] Filter out Users in Room settings

# ***Messaging***

[x] ~~Frostmark for message rendering~~

[x] Rich Media Support: Sending images etc.

[x] Adding Emoji reactions

[x] Threaded conversations

[x] Reply to message

[x] Automatic sliding sync / pagination

[x] Option to ignore user

[x] Always use sliding sync, remove Load more button

[x] Improve UI (emoji picker, better threading)

# ***Notifications***

[x] Notifications support

[x] Settings for Notifications

[x] Notification for Errors?

# ***Settings***
## **Application**

[x] Set up Config

[x] Global Notification Rules: The NotificationSettings API in the SDK allows you to
 configure whether the user is notified for specific events (e.g., mentions, direct
 messages, room messages, encrypted messages).
## **Room**

### *1. General Profile & Appearance*

[x] Room Name & Topic: Read and update via room.name() / room.topic() and their respective state setters (room.set_name(), room.set_room_topic()).

[x] Room Avatar: Fetch the avatar using room.avatar_url() and upload a new one by sending an m.room.avatar state event.

### *2. Notification Settings*

[x] Push Rules & Muting: The SDK has a dedicated client.notification_settings() API. You can allow users to set the room to:
    - All Messages: Notify for every message.
    - Mentions & Keywords Only: Only notify if the user is pinged.
    - Mute: Completely suppress notifications for the room.

### *3. Membership & Access Control*

[x] Leave / Forget: Add buttons for room.leave() and room.forget() (once left).

[x] Manage Members: List participants (room.members()) and provide actions to:
    - Invite (room.invite_user_by_id())
    - Kick (room.kick_user())
    - Ban / Unban (room.ban_user(), room.unban_user())

[x] Join Rules: Modify the m.room.join_rules state event to toggle whether the room is Invite Only, Public, Knock (request to join), or Restricted (requires membership in a
specific Space).

### *4. Security & Encryption*

[x] Enable E2E Encryption: If the room is unencrypted, you can add a button to call room.enable_encryption(). (Note: Make sure to warn the user this is a one-way action and
cannot be undone).

[x] History Visibility: Change the m.room.history_visibility state event to determine if new members can see past messages (Shared, Invited, or Joined).

### *5. Role & Permission Management*

[x] Power Levels: You can fetch the m.room.power_levels event and build a UI to promote/demote users (e.g., Default (0), Moderator (50), Admin (100)).

[x] Action Permissions: Use the power levels payload to let Admins restrict who can change the room name, invite new people, or send certain types of messages.

### *6. Advanced / Debug*

[x] Room ID & Aliases: Display the internal Matrix !room_id:server.com and manage its canonical aliases (#alias:server.com).

[x] Pinned Messages: Interface with the m.room.pinned_events state event to manage important messages.
## **Space**
### *1. Hierarchy & Children Management*

 [x] Add/Remove Rooms: Manage the m.space.child state events to link rooms to the Space.

 [x] Set Suggestions: Mark certain children as "suggested" so they are highlighted to new Space members.

 [x] Ordering: Define the order string in the child event to control how rooms appear in the Space sidebar.

 ### *2. Space-Restricted Join Rules*

[x] Restricted Access: You can update a room's join rules to m.room.join_rules: restricted, allowing anyone who is a member of the Space to join the room without an explicit
 invite.

 ### *3. Basic Profile (Same as Rooms)*

[x] Space Name & Topic: Updating the "Title" and "Description" of the Space.

[x] Space Avatar: Changing the icon that represents the Space in the switcher.

 ### *4. Visibility & Discovery*

[x] Canonical Alias: Manage the #space_name:server.com alias so the Space can be easily found and shared.

[x] Public vs. Private: Toggle whether the Space is discoverable in the server's directory or requires an invitation.
## **User**
### *1. Profile Management (client.account())*

[x] Display Name: Read and update the user's global display name (get_display_name, set_display_name).

[x] Avatar: Upload and change the user's global profile picture (get_avatar_url, upload_avatar, set_avatar_url).

### *2. Account & Security (client.account() / client.devices())*

[x] Session / Device Management: List all active logins, rename them, or log them out remotely (client.devices(), client.rename_device(), client.delete_devices()).

[x] Change Password: Allow the user to update their account password (change_password()).

[x] Emails and Phone Numbers (3PIDs): View, add, and remove linked emails or phone numbers (get_3pids(), add_3pid(), delete_3pid()).

[x] Deactivate Account: Allow users to permanently delete their matrix account (deactivate()).

### *3. Notifications & Push Rules (client.notification_settings())*

[x] Keywords: Manage custom keywords the user wants to be notified for.

### *4. Privacy & Preferences (client.account())*

[x] Ignored Users: View and manage the list of blocked/ignored users (ignore_user(), unignore_user(), is_user_ignored()).

[x] Media & URL Previews: Toggle global policies for displaying media previews and
 invite avatars (set_media_previews_display_policy(),
 set_invite_avatars_display_policy()).

### *5. Cross-Signing & Encryption (client.encryption())*

[x] Key Verification: Check cross-signing status, bootstrap cross-signing, or display the user's master/self-signing keys.

[x] Session Verification: Trigger or accept interactive verification requests for new devices.
