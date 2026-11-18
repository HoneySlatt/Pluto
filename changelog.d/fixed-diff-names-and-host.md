- Diffs show the names of files holding a space or a letter outside ASCII as they are (`my
  file.txt`, `été.txt`), not with git's trailing tab or octal quoting, in renames and binary files
  too.
- Images and uploads send the server's port in their `Host` header, as HTTP requires, so a server
  on another port than 80 or 443 that checks it answers them.
