#include <stdio.h>
static long a[4096],b[4096],c[4096];
int main(){ long n=64;
  for(long i=0;i<n;i++)for(long j=0;j<n;j++){a[i*n+j]=(i+j)%7;b[i*n+j]=(i*j)%5;}
  for(long i=0;i<n;i++)for(long j=0;j<n;j++){long s=0;for(long k=0;k<n;k++)s+=a[i*n+k]*b[k*n+j];c[i*n+j]=s;}
  long sum=0;for(long t=0;t<n*n;t++)sum+=c[t];
  printf("%ld\n",sum); return 0; }
