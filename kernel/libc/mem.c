#include "mem.h"
#include "paging.h"

void *
memset (void *dest, register int val, register size_t len)
{
  register unsigned char *ptr = (unsigned char*)dest;
  while (len-- > 0)
    *ptr++ = val;
  return dest;
}

int
memcmp (const void *str1, const void *str2, size_t count)
{
  register const unsigned char *s1 = (const unsigned char*)str1;
  register const unsigned char *s2 = (const unsigned char*)str2;

  while (count-- > 0)
    {
      if (*s1++ != *s2++)
	  return s1[-1] < s2[-1] ? -1 : 1;
    }
  return 0;
}

void *malloc(size_t size) {
    size_t pages = size / 4096;
    int extra = size % 4096;
    if (extra)
        pages++;
    void *first_page = alloc_page();
    for (size_t i = 1; i < pages; i++)
        alloc_page();
    return first_page;
}

void free(void*) {}

void *realloc(void*, size_t size) { return malloc(size); }