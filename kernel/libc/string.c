#include "string.h"

size_t strlen(const char *str) {
        const char *s;
        for (s = str; *s; ++s)
                ;
        return (s - str);
}
int
itoa(long long num, char* str, int len, int base)
{
        long long sum = num;
        int i = 0;
        int digit;
        if (len == 0)
                return -1;
        do
        {
                digit = sum % base;
                if (digit < 0xA)
                        str[i++] = '0' + digit;
                else
                        str[i++] = 'A' + digit - 0xA;
                sum /= base;
        }while (sum && (i < (len - 1)));
        if (i == (len - 1) && sum)
                return -1;
        str[i] = '\0';
        strrev(str);
        return 0;
}
void
strrev(char *str)
{
        int i;
        int j;
        char a;
        unsigned len = strlen(str);
        for (i = 0, j = len - 1; i < j; i++, j--)
        {
                a = str[i];
                str[i] = str[j];
                str[j] = a;
        }
}

int
strcmp (const char *p1, const char *p2)
{
  const unsigned char *s1 = (const unsigned char *) p1;
  const unsigned char *s2 = (const unsigned char *) p2;
  unsigned char c1, c2;
  do
    {
      c1 = (unsigned char) *s1++;
      c2 = (unsigned char) *s2++;
      if (c1 == '\0')
	return c1 - c2;
    }
  while (c1 == c2);
  return c1 - c2;
}